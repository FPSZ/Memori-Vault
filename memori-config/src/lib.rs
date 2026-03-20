use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("配置文件读取失败: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("YAML解析失败: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("配置验证失败: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub embedding: EmbeddingConfig,
    pub indexing: IndexingConfig,
    pub retrieval: RetrievalConfig,
    pub llm: LlmConfig,
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub database_url: String,
    pub pool_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite:memori.db".to_string(),
            pool_size: 4,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    pub model_name: String,
    pub dimensions: usize,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "nomic-embed-text".to_string(),
            dimensions: 768,
            api_url: None,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IndexingConfig {
    pub mode: IndexingMode,
    pub resource_budget: ResourceBudget,
    pub max_chunk_size: usize,
    pub overlap_size: usize,
    pub event_debounce_ms: u64,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            mode: IndexingMode::Continuous,
            resource_budget: ResourceBudget::Balanced,
            max_chunk_size: 1000,
            overlap_size: 200,
            event_debounce_ms: 500,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum IndexingMode {
    Continuous,
    Manual,
    Scheduled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum ResourceBudget {
    Low,
    Balanced,
    Fast,
}

impl ResourceBudget {
    pub fn graph_worker_idle_delay(&self) -> std::time::Duration {
        match self {
            ResourceBudget::Low => std::time::Duration::from_millis(650),
            ResourceBudget::Balanced => std::time::Duration::from_millis(260),
            ResourceBudget::Fast => std::time::Duration::from_millis(80),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub rrf_k: f64,
    pub min_score_threshold: f32,
    pub query_cache_size: usize,
    pub query_cache_ttl_secs: i64,
    pub dense_weight: f32,
    pub strict_lexical_weight: f32,
    pub broad_lexical_weight: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 20,
            rrf_k: 60.0,
            min_score_threshold: 0.3,
            query_cache_size: 256,
            query_cache_ttl_secs: 300,
            dense_weight: 1.0,
            strict_lexical_weight: 0.5,
            broad_lexical_weight: 0.3,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model_name: String,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
    pub max_tokens: usize,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Ollama,
            model_name: "qwen2.5".to_string(),
            api_url: None,
            api_key: None,
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum LlmProvider {
    Ollama,
    OpenAI,
    OpenAICompatible,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout_secs: u64,
    pub half_open_max_calls: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_secs: 30,
            half_open_max_calls: 3,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_paths(&[
            Self::default_config_path(),
            Self::user_config_path(),
            Self::local_config_path(),
        ])
    }

    pub fn load_from_paths<T: AsRef<std::path::Path>>(paths: &[T]) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        for path in paths {
            let path = path.as_ref();
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                let file_config: Config = serde_yaml::from_str(&content)?;
                config.merge(file_config);
            }
        }

        config.apply_env_overrides();
        config.validate()?;

        Ok(config)
    }

    fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("memori-vault")
            .join("config.yaml")
    }

    fn user_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("memori")
            .join("config.yaml")
    }

    fn local_config_path() -> PathBuf {
        PathBuf::from("config.yaml")
    }

    fn merge(&mut self, other: Config) {
        if other.server.host != Self::default().server.host {
            self.server.host = other.server.host;
        }
        if other.server.port != Self::default().server.port {
            self.server.port = other.server.port;
        }
        if other.storage.database_url != Self::default().storage.database_url {
            self.storage.database_url = other.storage.database_url;
        }
        if other.storage.pool_size != Self::default().storage.pool_size {
            self.storage.pool_size = other.storage.pool_size;
        }
        if other.embedding.model_name != Self::default().embedding.model_name {
            self.embedding.model_name = other.embedding.model_name;
        }
        if other.embedding.dimensions != Self::default().embedding.dimensions {
            self.embedding.dimensions = other.embedding.dimensions;
        }
        if other.retrieval.top_k != Self::default().retrieval.top_k {
            self.retrieval.top_k = other.retrieval.top_k;
        }
        if other.retrieval.rrf_k != Self::default().retrieval.rrf_k {
            self.retrieval.rrf_k = other.retrieval.rrf_k;
        }
        if other.circuit_breaker.failure_threshold != 0 {
            self.circuit_breaker.failure_threshold = other.circuit_breaker.failure_threshold;
        }
        if other.circuit_breaker.recovery_timeout_secs != 0 {
            self.circuit_breaker.recovery_timeout_secs = other.circuit_breaker.recovery_timeout_secs;
        }
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(host) = std::env::var("MEMORI_SERVER_HOST") {
            self.server.host = host;
        }
        if let Ok(port) = std::env::var("MEMORI_SERVER_PORT") {
            if let Ok(port) = port.parse() {
                self.server.port = port;
            }
        }
        if let Ok(url) = std::env::var("MEMORI_DATABASE_URL") {
            self.storage.database_url = url;
        }
        if let Ok(url) = std::env::var("MEMORI_EMBEDDING_URL") {
            self.embedding.api_url = Some(url);
        }
        if let Ok(key) = std::env::var("MEMORI_EMBEDDING_API_KEY") {
            self.embedding.api_key = Some(key);
        }
        if let Ok(url) = std::env::var("MEMORI_LLM_URL") {
            self.llm.api_url = Some(url);
        }
        if let Ok(key) = std::env::var("MEMORI_LLM_API_KEY") {
            self.llm.api_key = Some(key);
        }
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.retrieval.rrf_k <= 0.0 {
            return Err(ConfigError::ValidationError(
                "rrf_k must be positive".to_string(),
            ));
        }
        if self.retrieval.min_score_threshold < 0.0 || self.retrieval.min_score_threshold > 1.0 {
            return Err(ConfigError::ValidationError(
                "min_score_threshold must be between 0.0 and 1.0".to_string(),
            ));
        }
        if self.retrieval.top_k == 0 {
            return Err(ConfigError::ValidationError(
                "top_k must be greater than 0".to_string(),
            ));
        }
        if self.storage.pool_size == 0 {
            return Err(ConfigError::ValidationError(
                "pool_size must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            embedding: EmbeddingConfig::default(),
            indexing: IndexingConfig::default(),
            retrieval: RetrievalConfig::default(),
            llm: LlmConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.retrieval.top_k, 20);
        assert_eq!(config.retrieval.rrf_k, 60.0);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        config.retrieval.rrf_k = -1.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_resource_budget_delay() {
        assert_eq!(ResourceBudget::Low.graph_worker_idle_delay().as_millis(), 650);
        assert_eq!(ResourceBudget::Balanced.graph_worker_idle_delay().as_millis(), 260);
        assert_eq!(ResourceBudget::Fast.graph_worker_idle_delay().as_millis(), 80);
    }
}
