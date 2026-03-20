use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
    pub half_open_max_calls: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
        }
    }
}

impl CircuitBreakerConfig {
    pub fn new(failure_threshold: u32, recovery_timeout_secs: u64, half_open_max_calls: usize) -> Self {
        Self {
            failure_threshold,
            recovery_timeout: Duration::from_secs(recovery_timeout_secs),
            half_open_max_calls,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: AtomicState,
    failure_count: AtomicU32,
    last_failure_time: AtomicU64,
    half_open_calls: AtomicU32,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Copy)]
enum AtomicState {
    Closed,
    Open,
    HalfOpen,
}

impl From<CircuitState> for AtomicState {
    fn from(state: CircuitState) -> Self {
        match state {
            CircuitState::Closed => AtomicState::Closed,
            CircuitState::Open => AtomicState::Open,
            CircuitState::HalfOpen => AtomicState::HalfOpen,
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: AtomicState::Closed.into(),
            failure_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            half_open_calls: AtomicU32::new(0),
            config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    pub fn state(&self) -> CircuitState {
        match self.state.load(Ordering::SeqCst) {
            AtomicState::Closed => CircuitState::Closed,
            AtomicState::Open => CircuitState::Open,
            AtomicState::HalfOpen => CircuitState::HalfOpen,
        }
    }

    fn should_attempt_recovery(&self) -> bool {
        let last_failure = self.last_failure_time.load(Ordering::SeqCst);
        if last_failure == 0 {
            return true;
        }
        let elapsed = Duration::from_secs(last_failure);
        elapsed >= self.config.recovery_timeout
    }

    pub fn is_available(&self) -> bool {
        let state = self.state();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => self.should_attempt_recovery(),
            CircuitState::HalfOpen => {
                let calls = self.half_open_calls.load(Ordering::SeqCst);
                (calls as usize) < self.config.half_open_max_calls
            }
        }
    }

    pub fn record_success(&self) {
        let prev = self.state();
        if prev == CircuitState::HalfOpen {
            self.state.store(AtomicState::Closed, Ordering::SeqCst);
            self.failure_count.store(0, Ordering::SeqCst);
            self.half_open_calls.store(0, Ordering::SeqCst);
        } else if prev == CircuitState::Closed {
            self.failure_count.store(0, Ordering::SeqCst);
        }
    }

    pub fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        let now = Instant::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.last_failure_time.store(now, Ordering::SeqCst);

        if failures >= self.config.failure_threshold {
            self.state.store(AtomicState::Open, Ordering::SeqCst);
        }
    }

    pub fn record_half_open_call(&self) -> bool {
        let calls = self.half_open_calls.fetch_add(1, Ordering::SeqCst);
        (calls as usize) < self.config.half_open_max_calls
    }

    pub fn transition_to_half_open(&self) {
        self.state.store(AtomicState::HalfOpen, Ordering::SeqCst);
        self.half_open_calls.store(0, Ordering::SeqCst);
    }

    pub fn transition_to_open(&self) {
        self.state.store(AtomicState::Open, Ordering::SeqCst);
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[derive(Debug)]
pub struct CircuitBreakerResult<T> {
    pub result: Result<T, CircuitBreakerError>,
    pub state: CircuitState,
}

#[derive(Debug)]
pub enum CircuitBreakerError {
    Open,
    TooManyCalls,
    InnerError(String),
}

impl<T> CircuitBreakerResult<T> {
    pub fn success(value: T) -> Self {
        Self {
            result: Ok(value),
            state: CircuitState::Closed,
        }
    }

    pub fn open() -> Self {
        Self {
            result: Err(CircuitBreakerError::Open),
            state: CircuitState::Open,
        }
    }

    pub fn too_many_calls() -> Self {
        Self {
            result: Err(CircuitBreakerError::TooManyCalls),
            state: CircuitState::HalfOpen,
        }
    }

    pub fn inner_error<E: std::fmt::Display>(err: E) -> Self {
        Self {
            result: Err(CircuitBreakerError::InnerError(err.to_string())),
            state: CircuitState::Closed,
        }
    }

    pub fn is_success(&self) -> bool {
        self.result.is_ok()
    }

    pub fn is_open(&self) -> bool {
        matches!(self.result, Err(CircuitBreakerError::Open))
    }

    pub fn into_result(self) -> Result<T, CircuitBreakerError> {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_initial_state() {
        let cb = CircuitBreaker::with_defaults();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.is_available());
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::new(3, 60, 1));
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.is_available());
    }

    #[test]
    fn test_circuit_breaker_success_resets() {
        let cb = CircuitBreaker::with_defaults();
        
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_after_timeout() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::new(1, 0, 1));
        
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        assert!(cb.is_available());
    }

    #[test]
    fn test_half_open_max_calls() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::new(1, 0, 2));
        
        cb.record_failure();
        cb.transition_to_half_open();
        
        assert!(cb.record_half_open_call());
        assert!(cb.record_half_open_call());
        assert!(!cb.record_half_open_call());
    }
}
