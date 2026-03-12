use std::sync::Arc;

use memori_core::MemoriEngine;
use tokio::sync::Mutex;

pub(crate) struct DesktopState {
    pub(crate) engine: Arc<Mutex<Option<MemoriEngine>>>,
    pub(crate) init_error: Arc<Mutex<Option<String>>>,
}
