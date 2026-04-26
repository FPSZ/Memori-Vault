use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// 日志系统初始化 Guard，必须保持存活以维持非阻塞写入。
pub struct LogGuard {
    _non_blocking: tracing_appender::non_blocking::WorkerGuard,
}

/// 初始化结构化日志系统。
/// - 控制台输出：人类可读格式，带颜色
/// - 文件输出：JSON Lines 格式，按天轮转，位于 `log_dir/memori.log.YYYY-MM-DD`
/// - 日志级别：默认 `info`，可通过 `RUST_LOG` 环境变量覆盖
pub fn init_logging(log_dir: PathBuf) -> LogGuard {
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "memori.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 文件层：JSON 结构化，无 ANSI，带源码位置与线程 ID
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_thread_ids(true)
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .with_current_span(true)
        .with_ansi(false);

    // 控制台层：人类可读，带颜色
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .with_ansi(true);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "info,\
             memori_vault=info,\
             memori_core=info,\
             memori_storage=info,\
             memori_parser=info,\
             memori_server=info,\
             memori_desktop=info",
        )
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    LogGuard {
        _non_blocking: guard,
    }
}
