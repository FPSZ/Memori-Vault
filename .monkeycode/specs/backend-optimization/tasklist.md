# 后端性能优化实施任务列表

## 阶段一：基础设施优化

- [x] 1. 创建 memori-config 配置模块
  - [x] 1.1 创建 `memori-config/Cargo.toml`
  - [x] 1.2 实现 `Config` 结构体和 YAML 解析
  - [x] 1.3 定义 `RetrievalConfig`、`CircuitBreakerConfig` 等子配置
  - [x] 1.4 实现配置文件加载逻辑（支持 env 覆盖）
  - [x] 1.5 创建默认配置文件 `config.yaml.example`

- [x] 2. 数据库连接池实现
  - [x] 2.1 在 `memori-storage/Cargo.toml` 添加 `deadpool-sqlite` 依赖
  - [x] 2.2 创建 `memori-storage/src/pool.rs` 连接池模块
  - [ ] 2.3 修改 `Storage` 结构体使用连接池
  - [ ] 2.4 修改所有数据库操作使用 `execute` 方法
  - [ ] 2.5 更新 `ensure_table_column` 等函数使用连接池

- [ ] 3. 检查点 - 确保存储层测试通过
  - 确保 `cargo test -p memori-storage` 全部通过

## 阶段二：锁粒度优化

- [ ] 4. Server 状态管理重构
  - [ ] 4.1 修改 `ServerState` 使用 `RwLock<MemoriEngine>`
  - [ ] 4.2 修改 `sessions` 使用 `RwLock<HashMap>`
  - [ ] 4.3 添加只读访问方法 `read_state`
  - [ ] 4.4 添加写访问方法 `write_state`
  - [ ] 4.5 更新所有 handler 使用新的锁模式

- [ ] 5. Core 引擎并发优化
  - [ ] 5.1 分析 `MemoriEngine` 的只读/可写操作
  - [ ] 5.2 提取只读方法到 `EngineReadOnly` trait
  - [ ] 5.3 实现只读引用返回，避免锁竞争

- [ ] 6. 检查点 - 确保并发测试通过
  - 确保 `cargo test -p memori-server` 全部通过

## 阶段三：向量搜索优化

- [ ] 7. 创建 memori-vector-index 模块
  - [ ] 7.1 创建 `memori-vector-index/Cargo.toml`
  - [ ] 7.2 实现 `VectorIndex` trait
  - [ ] 7.3 实现 `HnswIndex` 结构体（使用 `greedy` crate）
  - [ ] 7.4 实现 `IndexBuilder` 索引构建器
  - [ ] 7.5 实现 `Searcher` 搜索器

- [ ] 8. 集成 HNSW 到 Storage 层
  - [ ] 8.1 修改 `SqliteStore` 添加 `hnsw_index` 字段
  - [ ] 8.2 修改 `insert_chunks` 同步更新 HNSW 索引
  - [ ] 8.3 修改 `search_similar_scoped` 使用 HNSW 搜索
  - [ ] 8.4 实现索引持久化（加载/保存）

- [ ] 9. 检查点 - 确保向量搜索功能正常
  - 确保检索结果一致性
  - 性能基准测试：10000 docs < 50ms

## 阶段四：熔断机制

- [ ] 10. 实现 CircuitBreaker
  - [ ] 10.1 创建 `memori-core/src/circuit_breaker.rs`
  - [ ] 10.2 实现 `CircuitBreaker` 结构体
  - [ ] 10.3 实现状态转换逻辑（Closed/Open/HalfOpen）
  - [ ] 10.4 实现 `call` 方法包装异步操作
  - [ ] 10.5 添加熔断恢复逻辑

- [ ] 11. 集成熔断器到 LLM 调用
  - [ ] 11.1 修改 `LlmGenerator` 添加 `circuit_breaker` 字段
  - [ ] 11.2 修改 `generate` 方法使用熔断器
  - [ ] 11.3 修改 `graph_extractor` 使用熔断器
  - [ ] 11.4 配置熔断器参数（从 config 读取）

- [ ] 12. 检查点 - 确保熔断器工作正常
  - 模拟 LLM 故障，验证降级响应

## 阶段五：配置迁移

- [ ] 13. 迁移硬编码常量到配置
  - [ ] 13.1 迁移 `RRF_K` 到 `RetrievalConfig`
  - [ ] 13.2 迁移 `QUERY_EMBEDDING_CACHE_SIZE/TLL` 到配置
  - [ ] 13.3 迁移 `MAX_CHUNK_SIZE`/`OVERLAP_SIZE` 到配置
  - [ ] 13.4 迁移 `top_k` 到配置
  - [ ] 13.5 迁移图谱 worker 延迟配置

- [ ] 14. 配置验证和错误处理
  - [ ] 14.1 添加配置验证逻辑
  - [ ] 14.2 添加缺失配置的默认值
  - [ ] 14.3 添加配置错误友好提示

## 阶段六：核心文件拆分

- [ ] 15. 拆分 engine.rs
  - [ ] 15.1 创建 `memori-core/src/engine/` 目录
  - [ ] 15.2 拆分 `search.rs` 搜索引擎
  - [ ] 15.3 拆分 `ask.rs` 问答处理
  - [ ] 15.4 拆分 `state.rs` 状态管理
  - [ ] 15.5 更新 `lib.rs` 导出

- [ ] 16. 拆分 retrieval.rs
  - [ ] 16.1 创建 `memori-core/src/retrieval/` 目录
  - [ ] 16.2 拆分 `scorer.rs` 评分器
  - [ ] 16.3 拆分 `router.rs` 文档路由
  - [ ] 16.4 拆分 `cache.rs` 缓存管理
  - [ ] 16.5 更新 `lib.rs` 导出

- [ ] 17. 拆分 server/main.rs
  - [ ] 17.1 创建 `memori-server/src/routes/` 目录
  - [ ] 17.2 拆分 `ask.rs` 路由
  - [ ] 17.3 拆分 `indexing.rs` 路由
  - [ ] 17.4 拆分 `model.rs`/`admin.rs`/`policy.rs` 路由
  - [ ] 17.5 创建 `middleware/` 目录
  - [ ] 17.6 拆分认证/指标/审计中间件

## 阶段七：集成与测试

- [ ] 18. 全模块集成
  - [ ] 18.1 更新 `memori-core/Cargo.toml` 依赖
  - [ ] 18.2 更新 `memori-server/Cargo.toml` 依赖
  - [ ] 18.3 更新 `memori-desktop/Cargo.toml` 依赖
  - [ ] 18.4 更新 workspace `Cargo.toml` members

- [ ] 19. 端到端测试
  - [ ] 19.1 运行 `cargo test --workspace`
  - [ ] 19.2 运行 `cargo clippy --workspace -- -D warnings`
  - [ ] 19.3 验证检索结果一致性
  - [ ] 19.4 性能基准测试

- [ ] 20. 最终检查
  - [ ] 20.1 确认 engine.rs < 500 行
  - [ ] 20.2 确认 retrieval.rs < 500 行
  - [ ] 20.3 确认 server/main.rs < 500 行
  - [ ] 20.4 提交所有更改
