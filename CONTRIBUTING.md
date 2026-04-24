# Contributing to Memori-Vault（中文）

感谢贡献。

Memori-Vault 是一个本地优先的记忆系统，支持桌面版与服务端模式。
如有取舍冲突，优先保证：正确性、可观测性、首问速度。

English version: [CONTRIBUTING.en.md](./CONTRIBUTING.en.md)

## 1. 先读文档

- 产品概览：`README.md`
- 英文概览：`README.zh-CN.md`
- 工程规则：`AI.md`
- UI 基线：`UI.md`

## 2. 仓库结构

- `memori-vault`：监听/防抖/事件通道
- `memori-parser`：解析与分块
- `memori-storage`：SQLite 持久化与检索
- `memori-core`：编排、检索链路、后台索引 worker
- `memori-desktop`：Tauri IPC 壳层
- `memori-server`：Axum HTTP 壳层
- `ui`：React + Vite + Tailwind v4

## 3. 环境要求

- Rust stable（建议 `1.85+`）
- Node.js 20+
- npm
- Ollama（本地模型模式需要）

## 4. 必过质量门

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm --dir ui run build
```

## 5. 本地开发

桌面模式：

```bash
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop
```

服务端/浏览器模式：

```bash
cargo run -p memori-server
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## 6. 运行时原则

1. 图谱失败不能拖垮检索主流程。
2. 首问不能被图谱全量构建阻塞。
3. 模型设置本地/远程 profile 必须隔离，不允许互相覆盖。
4. 桌面 IPC 与服务端 HTTP 语义保持一致。

## 7. 文档同步规则

只要行为或接口变更，必须同 PR 更新：
- `README.md` + `README.zh-CN.md`
- `CONTRIBUTING.md` + `CONTRIBUTING.zh-CN.md`
- `AI.md`（工程规则）
- `UI.md`（视觉交互基线变化时）

## 8. PR 检查清单

- 说明变更动机和边界。
- 记录 DTO/API/IPC 变化。
- 说明配置或存储迁移影响。
- 提供手动验证步骤。
- UI 变更附截图/GIF。

## 9. 安全与隐私

- 不提交本地数据库、模型文件、隐私文档、密钥。
- 禁止隐式遥测。
- 密钥与配置策略需可见、可审计、可迁移。
