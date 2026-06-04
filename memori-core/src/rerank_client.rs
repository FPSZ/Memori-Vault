use crate::resolve_runtime_model_config_from_env;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::warn;

const DEFAULT_RERANK_TIMEOUT_SECS: u64 = 20;
const RERANK_UNAVAILABLE_COOLDOWN_MS: i64 = 30_000;

#[derive(Debug, Error)]
pub enum RerankClientError {
    #[error("Rerank temporarily unavailable; skipping until cooldown expires")]
    TemporarilyUnavailable,

    #[error("Rerank request failed: {0}")]
    Request(#[source] reqwest::Error),

    #[error("Rerank service returned HTTP {status}, body: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("Rerank response deserialize failed: {0}")]
    Deserialize(#[source] reqwest::Error),

    #[error("Rerank service returned no scores (expected {expected})")]
    EmptyResults { expected: usize },
}

#[derive(Debug, Clone)]
pub struct LocalRerankClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    enabled: bool,
    unavailable_until_ms: Arc<AtomicI64>,
}

impl Default for LocalRerankClient {
    fn default() -> Self {
        let runtime = resolve_runtime_model_config_from_env();
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(DEFAULT_RERANK_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: runtime.rerank_endpoint,
            api_key: runtime.api_key,
            model: runtime.rerank_model,
            enabled: runtime.rerank_enabled,
            unavailable_until_ms: Arc::new(AtomicI64::new(0)),
        }
    }
}

impl LocalRerankClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, enabled: bool) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(DEFAULT_RERANK_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
            enabled,
            unavailable_until_ms: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.base_url.trim().is_empty()
    }

    pub(crate) fn should_attempt_at(&self, now_ms: i64) -> bool {
        self.is_enabled() && now_ms >= self.unavailable_until_ms.load(Ordering::Relaxed)
    }

    fn should_attempt_now(&self) -> bool {
        self.should_attempt_at(unix_now_ms())
    }

    pub(crate) fn mark_temporarily_unavailable(&self, now_ms: i64, cooldown_ms: i64) {
        self.unavailable_until_ms
            .store(now_ms.saturating_add(cooldown_ms.max(0)), Ordering::Relaxed);
    }

    fn mark_temporarily_unavailable_now(&self) {
        self.mark_temporarily_unavailable(unix_now_ms(), RERANK_UNAVAILABLE_COOLDOWN_MS);
    }

    fn clear_unavailable(&self) {
        self.unavailable_until_ms.store(0, Ordering::Relaxed);
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub async fn rerank(
        &self,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<f32>, RerankClientError> {
        if !self.should_attempt_now() {
            return Err(RerankClientError::TemporarilyUnavailable);
        }
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/v1/rerank", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(url).json(&RerankRequest {
            model: &self.model,
            query,
            documents,
            top_n: documents.len(),
        });
        if let Some(key) = self.api_key.as_ref() {
            request = request.bearer_auth(key);
        }

        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                if is_service_unavailable_request_error(&err) {
                    self.mark_temporarily_unavailable_now();
                }
                return Err(RerankClientError::Request(err));
            }
        };

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            if is_service_unavailable_status(status_code) {
                self.mark_temporarily_unavailable_now();
            }
            let body = response.text().await.unwrap_or_default();
            return Err(RerankClientError::HttpStatus {
                status: status_code,
                body,
            });
        }

        let parsed: RerankResponse = response
            .json()
            .await
            .map_err(RerankClientError::Deserialize)?;

        if parsed.results.is_empty() {
            return Err(RerankClientError::EmptyResults {
                expected: documents.len(),
            });
        }

        self.clear_unavailable();

        let mut scores = vec![f32::NEG_INFINITY; documents.len()];
        for item in parsed.results {
            if item.index < scores.len() {
                scores[item.index] = item.relevance_score;
            } else {
                warn!(
                    index = item.index,
                    len = documents.len(),
                    "rerank result index out of range; ignored"
                );
            }
        }
        Ok(scores)
    }
}

fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn is_service_unavailable_request_error(err: &reqwest::Error) -> bool {
    err.is_connect() || err.is_timeout() || err.is_request()
}

fn is_service_unavailable_status(status: u16) -> bool {
    matches!(status, 404 | 405 | 501 | 502 | 503 | 504)
}

#[derive(Debug, serde::Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: &'a [String],
    top_n: usize,
}

#[derive(Debug, serde::Deserialize)]
struct RerankResponse {
    #[serde(default)]
    results: Vec<RerankResultItem>,
}

#[derive(Debug, serde::Deserialize)]
struct RerankResultItem {
    index: usize,
    #[serde(alias = "score")]
    relevance_score: f32,
}

#[cfg(test)]
mod tests {
    use super::LocalRerankClient;

    #[test]
    fn client_enters_cooldown_after_unavailable_detection() {
        let client = LocalRerankClient::new("http://127.0.0.1:18004", "rerank", true);
        let now_ms = 1_000_i64;
        assert!(client.should_attempt_at(now_ms));

        client.mark_temporarily_unavailable(now_ms, 5_000);
        assert!(!client.should_attempt_at(now_ms));
        assert!(!client.should_attempt_at(now_ms + 4_999));
        assert!(client.should_attempt_at(now_ms + 5_000));
    }

    #[test]
    fn disabled_client_never_attempts_rerank() {
        let client = LocalRerankClient::new("http://127.0.0.1:18004", "rerank", false);
        assert!(!client.should_attempt_at(0));
    }
}
