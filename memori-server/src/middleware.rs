//! HTTP 横切中间件：request-id/trace 可观测性 + 按 IP 的接口限流。
//!
//! - `request_id_trace_middleware`：为每个请求分配/透传 `x-request-id`，并以该 id
//!   建立 tracing span，使检索链路日志可按请求聚合；响应回写同名头部便于端到端关联。
//! - `rate_limit_middleware`：按客户端 IP 做固定窗口限流，登录/管理类路径用更严阈值，
//!   防止暴力破解与管理面被刷。限流判定抽成纯函数 `RateLimiter::check`，便于单测。

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::Instrument;
use uuid::Uuid;

use crate::{ApiError, ServerState};

/// 请求关联 id 头部名（小写，HTTP/2 规范化后一致）。
pub(crate) const REQUEST_ID_HEADER: &str = "x-request-id";

/// 透传/生成 request-id，并以其建立 tracing span 包裹后续处理。
pub(crate) async fn request_id_trace_middleware(req: Request<Body>, next: Next) -> Response {
    let request_id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 200 && value.is_ascii())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
    );

    let mut response = next.run(req).instrument(span).await;
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER, header_value);
    }
    response
}

/// 按 IP 限流中间件：登录/管理类路径走严格桶，其余走宽松桶。
pub(crate) async fn rate_limit_middleware(
    State(state): State<ServerState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let limiter = &state.rate_limiter;
    if !limiter.enabled {
        return next.run(req).await;
    }

    let client_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0.ip())
        .unwrap_or(IpAddr::from([0, 0, 0, 0]));
    let bucket = RateBucket::for_path(req.uri().path());
    let now_ms = unix_now_millis();

    if !limiter.check(bucket, client_ip, now_ms) {
        let retry_after = limiter.window_secs(bucket);
        let mut response = ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "rate limit exceeded, please retry later".to_string(),
        }
        .into_response();
        if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
            response.headers_mut().insert("retry-after", value);
        }
        return response;
    }

    next.run(req).await
}

/// 限流桶类别：敏感（登录/管理）与普通分开计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RateBucket {
    Sensitive,
    General,
}

impl RateBucket {
    /// 登录与管理面用敏感桶（更低阈值），其余用普通桶。
    fn for_path(path: &str) -> Self {
        if path.starts_with("/api/auth") || path.starts_with("/api/admin") {
            Self::Sensitive
        } else {
            Self::General
        }
    }

    fn as_key(self) -> u8 {
        match self {
            Self::Sensitive => 0,
            Self::General => 1,
        }
    }
}

/// 固定窗口计数器：记录窗口起点与该窗口内已计数。
#[derive(Debug, Clone, Copy)]
struct WindowCounter {
    window_start_ms: u64,
    count: u32,
}

/// 单桶配置：窗口长度与窗口内允许的最大请求数。
#[derive(Debug, Clone, Copy)]
struct BucketConfig {
    window_ms: u64,
    max_requests: u32,
}

/// 按 IP 固定窗口限流器。`buckets` 以 (桶类别, IP) 为键独立计数。
pub(crate) struct RateLimiter {
    pub(crate) enabled: bool,
    sensitive: BucketConfig,
    general: BucketConfig,
    counters: Mutex<HashMap<(u8, IpAddr), WindowCounter>>,
}

impl RateLimiter {
    /// 从环境变量解析配置；缺省值：普通 600/min、敏感 20/min、默认开启。
    pub(crate) fn from_env() -> Self {
        let enabled = !matches!(
            std::env::var("MEMORI_RATE_LIMIT_ENABLED")
                .ok()
                .map(|value| value.trim().to_ascii_lowercase()),
            Some(ref value) if value == "0" || value == "false" || value == "off" || value == "no"
        );
        let general_max = parse_env_u32("MEMORI_RATE_LIMIT_PER_MIN", 600);
        let sensitive_max = parse_env_u32("MEMORI_AUTH_RATE_LIMIT_PER_MIN", 20);
        Self::new(enabled, general_max, sensitive_max)
    }

    pub(crate) fn new(enabled: bool, general_max: u32, sensitive_max: u32) -> Self {
        Self {
            enabled,
            sensitive: BucketConfig {
                window_ms: 60_000,
                max_requests: sensitive_max.max(1),
            },
            general: BucketConfig {
                window_ms: 60_000,
                max_requests: general_max.max(1),
            },
            counters: Mutex::new(HashMap::new()),
        }
    }

    fn config(&self, bucket: RateBucket) -> BucketConfig {
        match bucket {
            RateBucket::Sensitive => self.sensitive,
            RateBucket::General => self.general,
        }
    }

    fn window_secs(&self, bucket: RateBucket) -> u64 {
        (self.config(bucket).window_ms / 1000).max(1)
    }

    /// 判定并计数：返回 true 表示放行，false 表示超限。
    /// 固定窗口——窗口过期则重置计数；同时顺手清理其它过期条目防止无限膨胀。
    pub(crate) fn check(&self, bucket: RateBucket, ip: IpAddr, now_ms: u64) -> bool {
        let config = self.config(bucket);
        let mut counters = match self.counters.lock() {
            Ok(guard) => guard,
            // 锁中毒不应让服务拒绝所有请求；保守放行。
            Err(poisoned) => poisoned.into_inner(),
        };

        // 惰性清理：条目数偏大时清掉已过期窗口，避免 IP 漂移导致表无限增长。
        if counters.len() > 4096 {
            counters.retain(|_, counter| {
                now_ms.saturating_sub(counter.window_start_ms) < config.window_ms
            });
        }

        let entry = counters
            .entry((bucket.as_key(), ip))
            .or_insert(WindowCounter {
                window_start_ms: now_ms,
                count: 0,
            });
        if now_ms.saturating_sub(entry.window_start_ms) >= config.window_ms {
            entry.window_start_ms = now_ms;
            entry.count = 0;
        }
        if entry.count >= config.max_requests {
            return false;
        }
        entry.count += 1;
        true
    }
}

fn parse_env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn unix_now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, n])
    }

    #[test]
    fn allows_up_to_limit_then_blocks_within_window() {
        let limiter = RateLimiter::new(true, 600, 3);
        let now = 1_000_000;
        // 敏感桶上限 3：前 3 个放行，第 4 个超限。
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now));
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now + 1));
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now + 2));
        assert!(!limiter.check(RateBucket::Sensitive, ip(1), now + 3));
    }

    #[test]
    fn window_resets_after_expiry() {
        let limiter = RateLimiter::new(true, 600, 2);
        let now = 1_000_000;
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now));
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now + 1));
        assert!(!limiter.check(RateBucket::Sensitive, ip(1), now + 2));
        // 跨过 60s 窗口后重新放行。
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now + 60_001));
    }

    #[test]
    fn buckets_and_ips_are_independent() {
        let limiter = RateLimiter::new(true, 600, 1);
        let now = 1_000_000;
        // 同 IP 不同桶互不影响。
        assert!(limiter.check(RateBucket::Sensitive, ip(1), now));
        assert!(limiter.check(RateBucket::General, ip(1), now));
        assert!(!limiter.check(RateBucket::Sensitive, ip(1), now + 1));
        // 不同 IP 互不影响。
        assert!(limiter.check(RateBucket::Sensitive, ip(2), now + 1));
    }

    #[test]
    fn sensitive_paths_use_sensitive_bucket() {
        assert_eq!(
            RateBucket::for_path("/api/auth/oidc/login"),
            RateBucket::Sensitive
        );
        assert_eq!(
            RateBucket::for_path("/api/admin/metrics"),
            RateBucket::Sensitive
        );
        assert_eq!(RateBucket::for_path("/api/ask"), RateBucket::General);
        assert_eq!(RateBucket::for_path("/api/health"), RateBucket::General);
    }
}
