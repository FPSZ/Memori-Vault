use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use memori_core::MemoriEngine;
use tokio::sync::Mutex;

use crate::Role;

#[derive(Clone)]
pub(crate) struct ServerState {
    pub(crate) engine: Arc<Mutex<Option<MemoriEngine>>>,
    pub(crate) init_error: Arc<Mutex<Option<String>>>,
    pub(crate) sessions: Arc<Mutex<HashMap<String, SessionInfo>>>,
    pub(crate) metrics: Arc<ServerMetrics>,
    pub(crate) audit_file_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    pub(crate) subject: String,
    pub(crate) role: Role,
    pub(crate) issued_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(Debug, Default)]
pub(crate) struct ServerMetrics {
    pub(crate) total_requests: AtomicU64,
    pub(crate) failed_requests: AtomicU64,
    pub(crate) ask_requests: AtomicU64,
    pub(crate) ask_failed: AtomicU64,
    pub(crate) ask_latency_total_ms: AtomicU64,
}
