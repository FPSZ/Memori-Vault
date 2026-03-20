# 后端性能优化技术设计

## 1. 向量搜索优化 (HNSW)

### 1.1 架构设计

```rust
// 新增模块: memori-vector-index
memori-vector-index/
├── src/
│   ├── lib.rs          # 模块入口
│   ├── hnsw.rs         # HNSW 索引实现
│   ├── indexer.rs      # 索引构建器
│   └── searcher.rs     # 搜索器
```

### 1.2 核心接口

```rust
pub trait VectorIndex: Send + Sync {
    fn add(&mut self, id: i64, embedding: &[f32]) -> Result<(), IndexError>;
    fn search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>, IndexError>;
    fn remove(&mut self, id: i64) -> Result<(), IndexError>;
    fn len(&self) -> usize;
}

pub struct HnswIndex {
    dim: usize,
    max_elements: usize,
    m: usize,
    ef_construction: usize,
    inner: greedy::HnswIndex<f32>,
}
```

### 1.3 集成到 Storage 层

```rust
// memori-storage/src/vector.rs (修改)
pub struct SqliteStore {
    // ... 现有字段
    hnsw_index: RwLock<Option<Arc<dyn VectorIndex>>>,  // 新增
}
```

## 2. 锁粒度优化

### 2.1 架构设计

```rust
// memori-server/src/state.rs (修改)
pub struct ServerState {
    pub engine: Arc<RwLock<MemoriEngine>>,        // 改为 RwLock
    pub init_error: Arc<Mutex<Option<String>>>,  // 保留 Mutex (初始化错误不频繁访问)
    pub sessions: Arc<RwLock<HashMap<String, SessionInfo>>>,  // 改为 RwLock
    pub metrics: Arc<ServerMetrics>,
}

impl MemoriEngine {
    pub async fn read<U, F>(&self, f: impl FnOnce(&MemoriEngine) -> U) -> U {
        f(self)  // 引擎不可变，直接访问
    }
    
    pub async fn write<U, F>(&mut self, f: impl FnOnce(&mut MemoriEngine) -> U) -> U {
        f(self)  // 需要写操作时再获取写锁
    }
}
```

### 2.2 读写分离模式

```rust
// 读操作 - 并发
let engine = state.engine.read().await;
let result = engine.search(query).await;

// 写操作 - 独占
let mut engine = state.engine.write().await;
engine.rebuild_index().await?;
```

## 3. 数据库连接池

### 3.1 架构设计

```toml
# memori-storage/Cargo.toml (新增)
[dependencies]
deadpool-sqlite = "0.14"
```

```rust
// memori-storage/src/pool.rs (新增)
pub struct ConnectionPool {
    pool: deadpool_sqlite::Pool,
}

impl ConnectionPool {
    pub async fn new(database_url: &str, max_size: usize) -> Result<Self, StorageError> {
        let cfg = deadpool_sqlite::Config::new(database_url);
        let pool = cfg.create_pool(Some(deadpool_sqlite::Runtime::Tokio1), max_size)?;
        Ok(Self { pool })
    }
    
    pub async fn get(&self) -> Result<deadpool_sqlite::Object, StorageError> {
        self.pool.get().await.map_err(Into::into)
    }
}
```

### 3.2 Storage 层改造

```rust
// memori-storage/src/lib.rs (修改)
pub struct Storage {
    conn: ConnectionPool,  // 改为连接池
}

impl Storage {
    pub async fn execute<F, T>(&self, f: F) -> Result<T, StorageError>
    where
        F: FnOnce(&Connection) -> Result<T, SqliteError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.get().await?;
        let handle = tokio::task::spawn_blocking(move || f(&conn));
        handle.await.map_err(Into::into)?
    }
}
```

## 4. 配置中心化

### 4.1 配置模块

```rust
// 新增: memori-config/src/lib.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub embedding: EmbeddingConfig,
    pub indexing: IndexingConfig,
    pub retrieval: RetrievalConfig,
    pub llm: LlmConfig,
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub top_k: usize,                    // 之前: 硬编码
    pub rrf_k: f64,                      // 之前: const RRF_K = 60.0
    pub min_score_threshold: f32,        // 之前: 硬编码
    pub query_cache_size: usize,         // 之前: QUERY_EMBEDDING_CACHE_SIZE
    pub query_cache_ttl_secs: i64,       // 之前: QUERY_EMBEDDING_CACHE_TTL_SECS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,          // 熔断前允许的失败次数
    pub recovery_timeout_secs: u64,       // 熔断恢复时间
    pub half_open_max_calls: usize,      // 半开状态最大尝试次数
}
```

### 4.2 配置文件

```yaml
# config.yaml
server:
  host: "0.0.0.0"
  port: 8080
  
storage:
  database_url: "sqlite:memori.db"
  pool_size: 4

retrieval:
  top_k: 20
  rrf_k: 60.0
  min_score_threshold: 0.5
  query_cache_size: 256
  query_cache_ttl_secs: 300

circuit_breaker:
  failure_threshold: 5
  recovery_timeout_secs: 30
  half_open_max_calls: 3
```

## 5. 熔断机制

### 5.1 熔断器实现

```rust
// 新增: memori-core/src/circuit_breaker.rs
pub struct CircuitBreaker {
    state: AtomicState,
    failure_count: AtomicU32,
    last_failure_time: AtomicU64,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State { Closed, Open, HalfOpen }

impl CircuitBreaker {
    pub async fn call<F, Fut>(&self, op: F) -> Result<CircuitBreakerResult>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<String, LlmError>>,
    {
        match self.state() {
            State::Closed => {
                match op().await {
                    Ok(result) => {
                        self.on_success();
                        Ok(CircuitBreakerResult::Success(result))
                    }
                    Err(e) => {
                        self.on_failure();
                        if self.state() == State::Open {
                            Ok(CircuitBreakerResult::Degraded("系统繁忙，请稍后重试".to_string()))
                        } else {
                            Err(e)
                        }
                    }
                }
            }
            State::Open => Ok(CircuitBreakerResult::Degraded("服务暂时不可用".to_string())),
            State::HalfOpen => { /* 尝试调用 */ }
        }
    }
}
```

### 5.2 LLM 调用集成

```rust
// memori-core/src/llm_generator.rs (修改)
pub struct LlmGenerator {
    client: OllamaClient,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl LlmGenerator {
    pub async fn generate(&self, prompt: &str) -> Result<String, LlmError> {
        self.circuit_breaker.call(|| self.client.generate(prompt)).await
    }
}
```

## 6. 核心文件拆分

### 6.1 engine.rs 拆分

```
memori-core/src/
├── engine.rs              # 保留引擎入口和协调逻辑 (~300行)
├── engine/
│   ├── mod.rs             # 模块导出
│   ├── search.rs          # 搜索引擎 (~300行)
│   ├── ask.rs             # 问答处理 (~200行)
│   └── state.rs           # 引擎状态管理 (~200行)
```

### 6.2 retrieval.rs 拆分

```
memori-core/src/
├── retrieval.rs           # 保留主入口和融合逻辑 (~400行)
├── retrieval/
│   ├── mod.rs             # 模块导出
│   ├── scorer.rs          # 评分器 (~300行)
│   ├── router.rs          # 文档路由 (~300行)
│   └── cache.rs           # 缓存管理 (~200行)
```

### 6.3 server/main.rs 拆分

```
memori-server/src/
├── main.rs                # 保留入口和启动逻辑 (~300行)
├── routes/
│   ├── mod.rs             # 路由汇总
│   ├── ask.rs             # 问答 API (~200行)
│   ├── indexing.rs        # 索引 API (~200行)
│   ├── model.rs           # 模型 API (~150行)
│   ├── admin.rs           # 管理 API (~200行)
│   └── policy.rs          # 策略 API (~150行)
├── handlers/
│   ├── mod.rs
│   └── shared.rs          # 共享处理器
└── middleware/
    ├── mod.rs
    ├── auth.rs            # 认证中间件
    ├── metrics.rs         # 指标中间件
    └── audit.rs           # 审计中间件
```

## 7. 依赖关系

```
memori-config (新增)
    └── 被所有模块依赖

memori-vector-index (新增)
    └── memori-storage 依赖

memori-core
    ├── memori-config
    ├── memori-vector-index
    ├── memori-storage
    ├── memori-parser
    └──CircuitBreaker (内置)

memori-server
    └── memori-core
```
