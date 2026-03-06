Phase 1（Step 1+2）实施计划：Workspace 初始化与 Memori-Vault 打通
摘要
目标：在不涉及 UI 的前提下，完成 Headless Rust Core 的最小可运行骨架，并打通 memori-vault -> memori-core 的异步事件链路。
范围：仅覆盖 Phase 1 的第 1 步和第 2 步；memori-parser/memori-storage 先建边界与占位接口，不实现完整业务。
关键约束已锁定：四 crate 架构、递归监听仅 md/txt、500ms 防抖、tokio::mpsc 容量 8192、send().await 背压且不丢事件、thiserror 错误模型。
关键实现变更（决策完整）
Workspace 与 crate 拓扑
根工作区使用 Cargo workspace（Rust 2024 Edition），成员固定为：
memori-vault：文件监听与事件防抖。
memori-parser：解析/分块接口占位。
memori-storage：SQLite 实装起步 + 向量存储 trait 占位。
memori-core：主控引擎与事件消费中枢。
不创建 UI/Tauri 相关 crate；不引入前端依赖。
依赖策略（极简、仅当前阶段必需）
memori-vault：notify、tokio（time/sync/task）、tracing、thiserror。
memori-core：tokio、tracing、thiserror，并依赖 memori-vault、memori-parser、memori-storage。
memori-storage：rusqlite（先落 SQLite 通道）+ thiserror；向量后端仅定义 trait，不接 LanceDB/Qdrant 实现。
memori-parser：thiserror（及必要标准库）；仅定义 parser/chunker 接口与基础类型。
全局禁用 unwrap/expect，统一错误向上返回。
Memori-Vault 设计（Step 2 核心）
事件模型：定义 WatchEvent（Created/Modified/Removed/Renamed + PathBuf）。
监听规则：
递归监听目录。
仅接收 .md/.txt 文件事件，其他后缀直接过滤。
防抖与去重：
500ms 窗口。
同一路径在窗口内的重复 Modified 合并为单条事件。
合并后再投递到 channel，减少拥塞源。
通道策略：
tokio::sync::mpsc::channel(8192)。
生产端统一使用 send().await，通道满时背压等待，不丢事件。
若接收端关闭，返回明确错误并停止 memori_vault 任务（可观测日志）。
Core 引擎设计（与 Memori-Vault 打通）
对外统一入口：MemoriEngine。
状态模型：Arc<AppState> 预留共享状态（后续可挂 DB 连接与 Ollama 客户端句柄）。
事件消费：
MemoriEngine::start_daemon() 或 run() 为 async。
内部 tokio::spawn 持续消费 Receiver<WatchEvent>，使用 tracing 输出结构化日志。
错误模型：EngineError/MemoriVaultError 均采用 thiserror，边界清晰、可传播。
测试与验收
构建与静态校验
workspace cargo check 通过。
各 crate 能独立编译，不存在 UI 依赖泄漏。
Memori-Vault 行为测试
临时目录下快速多次修改同一 *.md 文件：500ms 内仅投递 1 条合并事件。
修改非 md/txt 文件：不投递事件。
大量连续事件（模拟 burst）：通道可持续接收，无静默丢失逻辑。
Core 打通测试
#[tokio::main] 测试入口（可放 examples）：启动 memori_vault + engine，触发文件变更后可在 core 侧收到并打印事件日志。
当 receiver 主动关闭时，memori_vault 能返回可诊断错误。
Step 1/2 验收标准（本轮）
已形成四 crate workspace。
目录监听、防抖、过滤、异步通道、core 消费链路全部可运行。
无 unwrap/expect，错误均结构化处理。
假设与默认值
当前以本机文件系统监听为目标（Windows 环境可运行），使用 notify 默认推荐 watcher。
memori-parser 与 memori-storage 在本轮仅提供可编译的接口/占位，不提前实现 Phase 1 Step 3/4 细节。
日志输出默认走 tracing 控制台层；后续可扩展到文件落盘与等级配置。

## Batch 1 增量实施（Step 1+2）- 已执行
- 已按“当前目录作为项目根目录”完成第一批工程落盘。
- 已新增文件：
  - /Cargo.toml（Workspace 配置）
  - /memori-vault/Cargo.toml
  - /memori-vault/src/lib.rs
- 已固化 Memori-Vault 关键约束：
  - 递归监听 + 仅 md/txt
  - 500ms 异步防抖
  - tokio::mpsc 容量 8192
  - send().await 背压等待，不丢事件
- 已实现 thiserror 错误模型与结构化事件类型（WatchEvent/WatchEventKind）。
- 工程性修正：
  - MemoriVaultHandle::join() 会先释放 watcher 再等待 worker，避免因回调发送端仍存活导致无法退出。

## 下一批实施目标（衔接）
- 创建 memori-parser / memori-storage / memori-core 最小可编译骨架。
- 在 memori-core 增加 MemoriEngine 与 Receiver 事件消费日志，打通 memori_vault 到 core。

## 当前验证状态
- 已执行：`cargo check -p memori-vault`
- 结果：失败（符合当前阶段现状）
- 原因：workspace 已声明 `memori-parser` / `memori-storage` / `memori-core`，但这三个 crate 尚未落盘，Cargo 无法加载缺失成员清单。
- 处理策略：下一批先补齐三个 crate 的最小骨架后再执行全量 `cargo check`。

## Batch 2 增量实施（Step 1+2）- 已执行
- 已新增 `memori-parser` 占位骨架：
  - `memori-parser/Cargo.toml`（仅依赖 `thiserror`）
  - `memori-parser/src/lib.rs`（`ParserStub` + `ParserError`）
- 已新增 `memori-storage` 占位骨架：
  - `memori-storage/Cargo.toml`（仅依赖 `thiserror`）
  - `memori-storage/src/lib.rs`（`StorageStub` + `StorageError`）
- 已新增 `memori-core` 并打通事件消费链路：
  - `memori-core/Cargo.toml`（依赖 `tokio/thiserror/tracing` 与本地 crates）
  - `memori-core/src/lib.rs`（`MemoriEngine` + `Arc<AppState>` + `start_daemon()` + `shutdown()`）
  - `memori-core/examples/daemon_demo.rs`（`#[tokio::main]` 演示入口）
- 已完成核心链路：`memori-vault` 产生文件事件 -> `memori-core` 异步消费并打印结构化日志。

## Batch 2 验证结果
- 已执行：`cargo check`
- 结果：通过（workspace 全部 crate 编译成功）
- 当前状态：Phase 1 Step 1+2 的工程骨架与事件消费链路已打通。
- 补充验证：cargo check --workspace --all-targets 通过（含 examples）。
