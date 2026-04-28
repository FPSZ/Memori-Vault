use std::collections::HashMap;
use std::process::Child;
use std::sync::Arc;

use memori_core::MemoriEngine;
use tokio::sync::Mutex;

pub(crate) struct DesktopState {
    pub(crate) engine: Arc<Mutex<Option<MemoriEngine>>>,
    pub(crate) init_error: Arc<Mutex<Option<String>>>,
    pub(crate) local_models: Arc<Mutex<HashMap<String, LocalModelProcess>>>,
}

pub(crate) struct LocalModelProcess {
    pub(crate) child: Child,
    pub(crate) endpoint: String,
    pub(crate) port: u16,
    pub(crate) model_path: String,
    pub(crate) model: String,
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
