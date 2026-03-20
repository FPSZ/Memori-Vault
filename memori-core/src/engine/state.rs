use super::*;

#[derive(Debug)]
pub struct EngineState {
    pub phase: String,
    pub last_error: Option<String>,
    pub total_indexed_files: u64,
    pub total_indexed_chunks: u64,
}

impl EngineState {
    pub fn new() -> Self {
        Self {
            phase: "idle".to_string(),
            last_error: None,
            total_indexed_files: 0,
            total_indexed_chunks: 0,
        }
    }

    pub fn set_phase(&mut self, phase: &str) {
        self.phase = phase.to_string();
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.last_error = error;
    }
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}
