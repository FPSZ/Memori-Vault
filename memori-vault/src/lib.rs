//! memori-vault
//!
//! 设计目标：
//! 1) 递归监听目录，仅关注 .md/.txt 文件；
//! 2) 在发送到核心层前进行 500ms 异步防抖与去重；
//! 3) 使用有界通道（8192）并在拥塞时 send().await 背压等待，严禁静默丢事件。
//!
//! 注意：本库不负责解析文本内容，只负责将文件系统变化标准化并可靠投递。

use notify::event::ModifyKind;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self, Instant, MissedTickBehavior};
use tracing::debug;

/// 默认防抖窗口：500ms（按架构要求固定为平衡档默认值）
pub const DEFAULT_DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

/// 事件通道容量：8192（按架构要求固定）
pub const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 8192;

/// 对外暴露的标准化文件事件类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEventKind {
    Created,
    Modified,
    Removed,
    Renamed,
}

/// 发送给核心层的统一事件结构。
#[derive(Debug, Clone)]
pub struct WatchEvent {
    /// 事件种类（创建/修改/删除/重命名）
    pub kind: WatchEventKind,
    /// 事件当前路径（重命名时为新路径）
    pub path: PathBuf,
    /// 重命名时的旧路径；非重命名事件为 None
    pub old_path: Option<PathBuf>,
    /// 观测时间（用于日志与调试）
    pub observed_at: SystemTime,
}

/// MemoriVault 运行配置。
#[derive(Debug, Clone)]
pub struct MemoriVaultConfig {
    /// 需要监听的根目录
    pub root: PathBuf,
    /// 防抖窗口（默认 500ms）
    pub debounce_window: Duration,
    /// 是否递归监听子目录（本项目默认 true）
    pub recursive: bool,
}

impl MemoriVaultConfig {
    /// 创建配置（默认递归 + 500ms 防抖）
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            debounce_window: DEFAULT_DEBOUNCE_WINDOW,
            recursive: true,
        }
    }
}

/// MemoriVault 层错误定义。
#[derive(Debug, Error)]
pub enum MemoriVaultError {
    /// notify watcher 初始化失败
    #[error("初始化文件监听器失败: {0}")]
    WatcherInit(#[source] notify::Error),

    /// watch(path) 注册失败
    #[error("监听目录失败: {path:?}, 原因: {source}")]
    WatchPath {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },

    /// notify 后端在运行中返回错误
    #[error("文件监听后端错误: {0}")]
    Backend(#[source] notify::Error),

    /// 下游消费者（memori-core）已关闭，事件无法继续发送
    #[error("事件通道已关闭，无法继续投递文件事件")]
    ChannelClosed,

    /// 后台任务 join 失败
    #[error("memori_vault 后台任务 join 失败: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),
}

/// 对外暴露的 memori_vault 句柄。
/// 持有 watcher 本体以保证监听生命周期；并持有异步 worker 任务句柄。
pub struct MemoriVaultHandle {
    watcher: Option<RecommendedWatcher>,
    worker: JoinHandle<Result<(), MemoriVaultError>>,
}

impl MemoriVaultHandle {
    /// 等待后台 worker 退出（通常用于测试或优雅停机）。
    /// 为了让 worker 可退出，会先主动释放 watcher。
    pub async fn join(mut self) -> Result<(), MemoriVaultError> {
        // 释放 watcher 后，notify 回调闭包被销毁，raw 发送端随之关闭，
        // worker 在收到通道关闭后会 flush 并正常退出。
        let _ = self.watcher.take();
        self.worker.await?
    }
}

/// 创建核心事件通道（强制容量 8192）。
pub fn create_event_channel() -> (mpsc::Sender<WatchEvent>, mpsc::Receiver<WatchEvent>) {
    mpsc::channel(DEFAULT_EVENT_CHANNEL_CAPACITY)
}

/// 启动 memori_vault：
/// 1) 用 notify 接收原始文件系统事件；
/// 2) 在 tokio 任务中做异步防抖与去重；
/// 3) 将结果发送到 out_tx（send().await，背压不丢）。
pub fn spawn_memori_vault(
    config: MemoriVaultConfig,
    out_tx: mpsc::Sender<WatchEvent>,
) -> Result<MemoriVaultHandle, MemoriVaultError> {
    // notify 回调在其内部线程触发；此处使用无界通道把事件桥接到 tokio 异步侧。
    // 该通道只用于“原始事件中转”，真正业务通道是下游 out_tx（有界 8192）。
    let (callback_tx, raw_rx) = mpsc::unbounded_channel::<RawMessage>();

    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let msg = match result {
                Ok(event) => RawMessage::FsEvent(event),
                Err(err) => RawMessage::BackendError(err),
            };

            // 如果发送失败，说明异步侧已经关闭；这里不 panic，交给上层生命周期管理。
            let _ = callback_tx.send(msg);
        },
        Config::default(),
    )
    .map_err(MemoriVaultError::WatcherInit)?;

    let mode = if config.recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };

    watcher
        .watch(&config.root, mode)
        .map_err(|source| MemoriVaultError::WatchPath {
            path: config.root.clone(),
            source,
        })?;

    // 异步 worker：负责防抖、去重、过滤、并 send().await 背压投递。
    let worker = tokio::spawn(run_debounce_loop(config.debounce_window, raw_rx, out_tx));

    Ok(MemoriVaultHandle {
        watcher: Some(watcher),
        worker,
    })
}

/// notify 回调到异步 worker 的内部消息。
enum RawMessage {
    FsEvent(Event),
    BackendError(notify::Error),
}

/// pending 队列中的缓存事件（用于防抖窗口聚合）。
#[derive(Debug, Clone)]
struct PendingEvent {
    event: WatchEvent,
    deadline: Instant,
}

/// 防抖 worker 主循环：
/// - 收到原始事件时写入 pending map（同路径覆盖，达到去重）
/// - 定时扫描到期事件并发送到 out_tx
async fn run_debounce_loop(
    debounce_window: Duration,
    mut raw_rx: mpsc::UnboundedReceiver<RawMessage>,
    out_tx: mpsc::Sender<WatchEvent>,
) -> Result<(), MemoriVaultError> {
    // key = path。语义：同一路径在窗口内重复变更只保留最新一条。
    let mut pending: HashMap<PathBuf, PendingEvent> = HashMap::new();

    // 轻量 tick，避免每个事件创建独立 sleep；50ms 粒度足够平衡 CPU 与实时性。
    let mut tick = time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            maybe_msg = raw_rx.recv() => {
                match maybe_msg {
                    Some(RawMessage::BackendError(err)) => {
                        return Err(MemoriVaultError::Backend(err));
                    }
                    Some(RawMessage::FsEvent(event)) => {
                        let now = Instant::now();
                        let normalized = normalize_notify_event(event);

                        for evt in normalized {
                            // 仅允许 .md/.txt 进入管线（满足本阶段范围约束）。
                            if !should_keep_event(&evt) {
                                continue;
                            }

                            // 去重核心：同一路径覆盖旧值，并刷新截止时间。
                            let key = evt.path.clone();
                            pending.insert(
                                key,
                                PendingEvent {
                                    event: evt,
                                    deadline: now + debounce_window,
                                },
                            );
                        }
                    }
                    None => {
                        // 上游原始事件关闭时，尽量把 pending 全部 flush 完毕再退出。
                        flush_ready_events(&mut pending, Instant::now(), true, &out_tx).await?;
                        return Ok(());
                    }
                }
            }
            _ = tick.tick() => {
                flush_ready_events(&mut pending, Instant::now(), false, &out_tx).await?;
            }
        }
    }
}

/// 把“已到期”或“强制全部”事件发送到下游。
async fn flush_ready_events(
    pending: &mut HashMap<PathBuf, PendingEvent>,
    now: Instant,
    flush_all: bool,
    out_tx: &mpsc::Sender<WatchEvent>,
) -> Result<(), MemoriVaultError> {
    let mut ready_keys = Vec::new();

    for (path, item) in pending.iter() {
        if flush_all || item.deadline <= now {
            ready_keys.push(path.clone());
        }
    }

    for key in ready_keys {
        if let Some(item) = pending.remove(&key) {
            // 关键要求：必须使用 send().await。
            // 当通道满时，生产端在这里异步等待（背压），不丢事件。
            out_tx
                .send(item.event)
                .await
                .map_err(|_| MemoriVaultError::ChannelClosed)?;
        }
    }

    Ok(())
}

/// 将 notify 原始事件映射为业务事件。
fn normalize_notify_event(event: Event) -> Vec<WatchEvent> {
    let observed_at = SystemTime::now();
    let Event { kind, paths, .. } = event;

    match kind {
        EventKind::Create(_) => paths
            .into_iter()
            .map(|path| WatchEvent {
                kind: WatchEventKind::Created,
                path,
                old_path: None,
                observed_at,
            })
            .collect(),

        EventKind::Modify(ModifyKind::Name(_)) => {
            // rename 常见是 [old, new]；若后端只给出一个路径，则退化为 new=old。
            let old_path = paths.first().cloned();
            let new_path = paths.get(1).cloned().or_else(|| old_path.clone());

            match new_path {
                Some(path) => vec![WatchEvent {
                    kind: WatchEventKind::Renamed,
                    path,
                    old_path,
                    observed_at,
                }],
                None => Vec::new(),
            }
        }

        EventKind::Modify(_) => paths
            .into_iter()
            .map(|path| WatchEvent {
                kind: WatchEventKind::Modified,
                path,
                old_path: None,
                observed_at,
            })
            .collect(),

        EventKind::Remove(_) => paths
            .into_iter()
            .map(|path| WatchEvent {
                kind: WatchEventKind::Removed,
                path,
                old_path: None,
                observed_at,
            })
            .collect(),

        // Access/Other 等噪声事件在本阶段忽略。
        _ => Vec::new(),
    }
}

/// 是否保留该事件进入业务通道。
/// 规则：当前路径或旧路径只要任一为 .md/.txt 即保留（兼容 rename 场景）。
fn should_keep_event(event: &WatchEvent) -> bool {
    if is_supported_text_file(&event.path) {
        return true;
    }
    if let Some(old) = &event.old_path {
        if is_supported_text_file(old) {
            return true;
        }
    }
    if matches!(
        event.kind,
        WatchEventKind::Removed | WatchEventKind::Renamed
    ) && path_has_no_extension(&event.path)
    {
        return true;
    }
    if matches!(event.kind, WatchEventKind::Renamed)
        && event
            .old_path
            .as_ref()
            .is_some_and(|old| path_has_no_extension(old))
    {
        return true;
    }
    false
}

/// 扩展名过滤：仅允许 md/txt（大小写不敏感）。
fn is_supported_text_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };

    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("txt")
}

fn path_has_no_extension(path: &Path) -> bool {
    path.extension().is_none()
}

/// 对外暴露的轻量诊断信息，可用于 core 层日志落点验证。
pub fn memori_vault_defaults_debug() {
    debug!(
        debounce_ms = DEFAULT_DEBOUNCE_WINDOW.as_millis() as u64,
        channel_capacity = DEFAULT_EVENT_CHANNEL_CAPACITY,
        "memori_vault default config loaded"
    );
}

#[cfg(test)]
mod tests {
    use super::{WatchEvent, WatchEventKind, should_keep_event};
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[test]
    fn keeps_removed_directory_event_for_cleanup() {
        let event = WatchEvent {
            kind: WatchEventKind::Removed,
            path: PathBuf::from("notes/project"),
            old_path: None,
            observed_at: SystemTime::now(),
        };

        assert!(should_keep_event(&event));
    }

    #[test]
    fn keeps_renamed_directory_event_for_cleanup() {
        let event = WatchEvent {
            kind: WatchEventKind::Renamed,
            path: PathBuf::from("notes/archive"),
            old_path: Some(PathBuf::from("notes/project")),
            observed_at: SystemTime::now(),
        };

        assert!(should_keep_event(&event));
    }
}
