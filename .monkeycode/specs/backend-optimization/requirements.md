# 后端性能优化需求

## 1. 背景与目标

### 现状问题
Memori-Vault 后端存在以下性能瓶颈：

1. **向量搜索瓶颈** - `memori-core/src/retrieval.rs` 中全内存遍历向量进行相似度计算
2. **锁竞争严重** - `memori-server/src/main.rs` 使用粗粒度 `Mutex<Option<MemoriEngine>>`
3. **数据库连接串行** - 单连接 + 锁持有时间长
4. **硬编码常量多** - 配置分散，难以调整
5. **缺乏熔断机制** - LLM调用失败直接返回错误

### 优化目标
- 向量搜索延迟降低 80%
- API响应时间 P99 降低 50%
- 支持 10000+ 文档规模
- 提高系统稳定性

## 2. 优化项

### 2.1 向量搜索优化 (HNSW索引)
- 引入 `greedy` 或 `fastembed` crate 实现 HNSW 向量索引
- 替换当前全内存遍历的 `search_similar_scoped` 实现
- 支持增量索引构建
- 向量维度：384/768/1024 可配置

### 2.2 锁粒度优化
- `Mutex<Option<MemoriEngine>>` 改为 `RwLock<MemoriEngine>`
- 拆分只读状态和可写状态
- 引擎初始化后不可变引用可并发访问

### 2.3 数据库连接池
- 引入 `deadpool-sqlite` 或 `r2d2-sqlite`
- 替换当前单连接模式
- 配置池大小：CPU核数或指定值

### 2.4 配置中心化
- 创建 `memori-config` 模块
- 统一管理所有配置常量
- 支持 YAML/JSON 配置文件
- 支持环境变量覆盖

### 2.5 熔断机制
- 引入 `熔断器模式` 处理 LLM 调用
- 配置失败阈值、重试间隔、熔断时长
- 失败时返回降级响应而非直接报错

### 2.6 核心文件拆分
- `memori-core/src/engine.rs` (869行) 拆分
- `memori-core/src/retrieval.rs` (1326行) 拆分
- `memori-server/src/main.rs` (2500行) 拆分

## 3. 验收标准

- [ ] 向量搜索 10000 docs 延迟 < 50ms
- [ ] API P99 响应时间 < 500ms
- [ ] 并发查询 100 req/s 无锁争用
- [ ] LLM 故障时系统返回降级响应
- [ ] 所有配置可通过配置文件修改
- [ ] 核心文件行数 < 500行
- [ ] 现有测试全部通过
