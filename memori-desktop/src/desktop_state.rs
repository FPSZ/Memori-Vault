use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;

use memori_core::MemoriEngine;
use serde::Serialize;
use tokio::sync::Mutex;

pub(crate) struct DesktopState {
    pub(crate) engine: Arc<Mutex<Option<MemoriEngine>>>,
    pub(crate) init_error: Arc<Mutex<Option<String>>>,
    pub(crate) local_models: Arc<Mutex<HashMap<String, LocalModelProcess>>>,
    pub(crate) regression_runs: Arc<Mutex<HashMap<String, RetrievalRegressionRunState>>>,
}

pub(crate) struct LocalModelProcess {
    pub(crate) child: Child,
    pub(crate) endpoint: String,
    pub(crate) port: u16,
    pub(crate) model_path: String,
    pub(crate) model: String,
    pub(crate) log_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RetrievalRegressionRunState {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) mode: String,
    pub(crate) profile: String,
    pub(crate) case_filter: Option<String>,
    pub(crate) started_at_ms: u128,
    pub(crate) finished_at_ms: Option<u128>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) report_path: Option<String>,
    pub(crate) stdout_tail: String,
    pub(crate) stderr_tail: String,
    pub(crate) error: Option<String>,
}

impl DesktopState {
    pub(crate) async fn stop_all_local_models(&self) {
        let processes = {
            let mut guard = self.local_models.lock().await;
            guard.drain().collect::<Vec<_>>()
        };
        for (role, mut process) in processes {
            let pid = process.child.id();
            match process.child.kill() {
                Ok(()) => {
                    let _ = process.child.wait();
                    tracing::info!(
                        role = %role,
                        pid = pid,
                        port = process.port,
                        "stopped local llama.cpp model during shutdown"
                    );
                }
                Err(err) => {
                    tracing::warn!(
                        role = %role,
                        pid = pid,
                        error = %err,
                        "failed to stop local llama.cpp model during shutdown"
                    );
                }
            }
        }
    }
}
