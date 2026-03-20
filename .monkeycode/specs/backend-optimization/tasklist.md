# 后端性能优化实施任务列表

## 阶段一：基础设施优化

- [x] 1. 创建 memori-config 配置模块
  - [x] 1.1 创建 `memori-config/Cargo.toml`
  - [x] 1.2 实现 `Config` 结构体和 YAML 解析
  - [x] 1.3 定义 `RetrievalConfig`、`CircuitBreakerConfig` 等子配置
  - [x] 1.4 实现配置文件加载逻辑（支持 env 覆盖）
  - [x] 1.5 创建默认配置文件 `config.yaml.example`

- [x] 2. 数据库连接池实现
  - [x] 2.1 创建 `memori-storage/src/pool.rs` 连接池模块
  - [x] 2.2 实现 `ConnectionPool` 和 `PooledConnection`
  - [ ] 2.3 保留向后兼容，Storage 继续使用 Mutex<Connection>
  - [ ] 2.4 渐进式改进，待后续集成

- [ ] 3. 检查点 - 确保存储层测试通过
  - 待 Rust 环境可用后执行 `cargo test -p memori-storage`

## 阶段二：锁粒度优化

- [x] 4. Server 状态管理重构
  - [x] 4.1 修改 `ServerState` 使用 `RwLock<Option<MemoriEngine>>`
  - [x] 4.2 修改 `sessions` 使用 `RwLock<HashMap>>`
  - [x] 4.3 添加 `read_engine/write_engine/read_sessions/write_sessions` helper方法
  - [x] 4.4 更新 `replace_engine` 使用 RwLock

- [ ] 5. Core 引擎并发优化
  - [ ] 5.1 分析 `MemoriEngine` 的只读/可写操作
  - [ ] 5.2 提取只读方法到 `EngineReadOnly` trait
  - [ ] 5.3 实现只读引用返回，避免锁竞争

- [ ] 6. 检查点 - 确保并发测试通过
  - 待 Rust 环境可用后执行 `cargo test -p memori-server`

## 阶段三：向量搜索优化

- [x] 7. 创建 memori-vector-index 模块
  - [x] 7.1 创建 `memori-vector-index/Cargo.toml`
  - [x] 7.2 实现 `VectorIndex` trait
  - [x] 7.3 实现简化版 `HnswIndex` 结构体
  - [x] 7.4 实现 `IndexBuilder` 索引构建器
  - [x] 7.5 提供 cosine similarity 搜索实现

- [ ] 8. 集成 HNSW 到 Storage 层
  - [ ] 8.1 修改 `SqliteStore` 添加 `hnsw_index` 字段
  - [ ] 8.2 修改 `insert_chunks` 同步更新 HNSW 索引
  - [ ] 8.3 修改 `search_similar_scoped` 使用 HNSW 搜索
  - [ ] 8.4 实现索引持久化（加载/保存）

- [ ] 9. 检查点 - 确保向量搜索功能正常
  - 待集成后执行性能基准测试

## 阶段四：熔断机制

- [x] 10. 实现 CircuitBreaker
  - [x] 10.1 创建 `memori-core/src/circuit_breaker.rs`
  - [x] 10.2 实现 `CircuitBreaker` 结构体
  - [x] 10.3 实现状态转换逻辑（Closed/Open/HalfOpen）
  - [x] 10.4 添加 `record_success/record_failure` 方法
  - [x] 10.5 完整的单元测试

- [ ] 11. 集成熔断器到 LLM 调用
  - [ ] 11.1 修改 `LlmGenerator` 添加 `circuit_breaker` 字段
  - [ ] 11.2 修改 `generate` 方法使用熔断器
  - [ ] 11.3 修改 `graph_extractor` 使用熔断器
  - [ ] 11.4 配置熔断器参数（从 config 读取）

- [ ] 12. 检查点 - 确保熔断器工作正常
  - 待集成后进行故障模拟测试

## 阶段五：配置迁移

- [x] 13. 配置模块基础
  - [x] 13.1 `RetrievalConfig` 包含 rrf_k, query_cache_size 等
  - [x] 13.2 添加 doc_top_k, chunk_candidate_k, final_answer_k 字段

- [ ] 14. 完整配置迁移
  - [ ] 14.1 迁移 `RRF_K` 到 `RetrievalConfig.rrf_k`
  - [ ] 14.2 迁移 `QUERY_EMBEDDING_CACHE_SIZE/TTL` 到配置
  - [ ] 14.3 迁移 `MAX_CHUNK_SIZE`/`OVERLAP_SIZE` 到配置
  - [ ] 14.4 迁移图谱 worker 延迟配置

## 阶段六：核心文件拆分

- [x] 15. engine.rs 拆分（预备）
  - [x] 15.1 创建 `memori-core/src/engine/` 目录
  - [x] 15.2 创建 `search.rs` 模块框架
  - [x] 15.3 创建 `ask.rs` 模块框架
  - [x] 15.4 创建 `state.rs` 模块框架
  - [ ] 15.5 逐步迁移 engine.rs 代码到子模块

- [ ] 16. retrieval.rs 拆分
  - [ ] 16.1 创建 `memori-core/src/retrieval/` 目录
  - [ ] 16.2 拆分 `scorer.rs` 评分器
  - [ ] 16.3 拆分 `router.rs` 文档路由
  - [ ] 16.4 拆分 `cache.rs` 缓存管理

- [ ] 17. server/main.rs 拆分
  - [ ] 17.1 创建 `memori-server/src/routes/` 目录
  - [ ] 17.2 拆分 `ask.rs` 路由
  - [ ] 17.3 拆分 `indexing.rs` 路由
  - [ ] 17.4 拆分 `model.rs`/`admin.rs`/`policy.rs` 路由
  - [ ] 17.5 创建 `middleware/` 目录
  - [ ] 17.6 拆分认证/指标/审计中间件

## 阶段七：集成与测试

- [ ] 18. 全模块集成
  - [x] 18.1 更新 workspace `Cargo.toml` members (添加 config, vector-index)
  - [ ] 18.2 更新 `memori-core/Cargo.toml` 依赖
  - [ ] 18.3 更新 `memori-server/Cargo.toml` 依赖
  - [ ] 18.4 更新 `memori-desktop/Cargo.toml` 依赖

- [ ] 19. 端到端测试
  - [ ] 19.1 运行 `cargo test --workspace`
  - [ ] 19.2 运行 `cargo clippy --workspace -- -D warnings`
  - [ ] 19.3 验证检索结果一致性
  - [ ] 19.4 性能基准测试

- [ ] 20. 最终检查
  - [ ] 20.1 确认 engine.rs < 500 行
  - [ ] 20.2 确认 retrieval.rs < 500 行
  - [ ] 20.3 确认 server/main.rs < 500 行
  - [x] 20.4 提交所有更改

## 进度总结

### 已完成
- 阶段一：memori-config 配置中心
- 阶段二：RwLock 锁粒度优化
- 阶段三：HNSW 向量索引基础实现
- 阶段四：CircuitBreaker 熔断器
- 阶段五：RetrievalConfig 配置扩展
- 阶段六：engine 模块目录结构

### 待完成
- 阶段三：HNSW 集成到 Storage
- 阶段四：熔断器集成到 LLM
- 阶段五：完整配置迁移
- 阶段六：retrieval.rs 和 server/main.rs 拆分
- 阶段七：集成测试

### 新增模块
```
memori-config/          # 配置中心
memori-vector-index/    # HNSW 向量索引
memori-core/src/engine/ # engine 模块拆分
memori-core/src/circuit_breaker.rs  # 熔断器
```
