# Contributing to Memori-Vault（中文）

感谢你为 Memori-Vault 贡献代码。

本项目是一个 Local-First 的个人记忆系统，核心目标是：在本地完成监听、解析、向量化、图谱抽取与检索，不依赖云端。

English version: [CONTRIBUTING.md](./CONTRIBUTING.md)

## 1. 贡献前先读

- 项目介绍（英文）：`README.md`
- 项目介绍（中文）：`README.zh-CN.md`
- 许可证：`LICENSE`（Apache-2.0）

## 2. 仓库结构

Rust workspace（根目录 `Cargo.toml`）当前包含：

- `memori-vault`：文件监听、异步防抖、背压事件通道（`.md/.txt`）
- `memori-parser`：文本规范化与语义分块
- `memori-storage`：SQLite 向量/图谱持久化与检索
- `memori-core`：引擎编排、daemon 生命周期、Ollama 调用
- `memori-desktop`：Tauri v2 桌面壳 + IPC 命令
- `memori-server`：Axum HTTP 服务（浏览器模式/无桌面壳）
- `ui`：React + Vite + Tailwind v4 前端
- `scripts`：本地 smoke 脚本（Windows PowerShell）

## 3. 环境要求

## 必需

- Rust stable（建议 `1.85+`）
- Node.js 20+
- npm
- Ollama（本地模型服务）

## Linux 构建 Tauri 额外依赖

与 CI 保持一致，至少需要（Ubuntu）：

- `pkg-config`
- `patchelf`
- `libglib2.0-dev`
- `libgirepository1.0-dev`
- `libgtk-3-dev`
- `libayatana-appindicator3-dev`
- `librsvg2-dev`
- `libsoup-3.0-dev`
- `libwebkit2gtk-4.1-dev`（或 `4.0-dev`，按发行版可用性）

## 4. 本地开发运行方式

## A. 仅跑 Rust 质量门（提交前最低要求）

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## B. 跑桌面版（Tauri + UI）

1. 安装前端依赖：

```bash
npm ci --prefix ui
```

2. 启动前端 dev server（端口固定 `1420`）：

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

3. 启动桌面壳（在仓库根目录执行）：

```bash
cargo tauri dev -p memori-desktop
```

如果本机未安装 `tauri-cli`，可用：

```bash
cargo run -p memori-desktop
```

## C. 跑浏览器模式（不依赖 Tauri 宿主）

1. 启动后端 HTTP 服务：

```bash
cargo run -p memori-server
```

2. 启动 UI：

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

3. 浏览器访问：`http://127.0.0.1:1420`

## D. Windows 一键 smoke

```powershell
pwsh -ExecutionPolicy Bypass -File .\scripts\smoke-start.ps1
pwsh -ExecutionPolicy Bypass -File .\scripts\smoke-stop.ps1
```

## 5. 常用环境变量

核心变量（按需设置）：

- `MEMORI_WATCH_ROOT`：监听目录
- `MEMORI_DB_PATH`：SQLite 路径（默认当前目录 `.memori.db`）
- `MEMORI_EMBED_MODEL`：embedding 模型（默认 `nomic-embed-text:latest`）
- `MEMORI_CHAT_MODEL`：回答模型（默认 `qwen2.5:7b`）
- `MEMORI_GRAPH_MODEL`：图谱抽取模型（默认 `qwen2.5:7b`）
- `MEMORI_OLLAMA_BASE_URL`：embedding 接口基地址（默认 `http://localhost:11434`）
- `MEMORI_OLLAMA_CHAT_ENDPOINT`：chat 接口地址（默认 `http://localhost:11434/api/chat`）
- `MEMORI_SERVER_ADDR`：`memori-server` 监听地址（默认 `127.0.0.1:3757`）
- `OLLAMA_MODELS`：Ollama 模型目录（常用于 Windows 自定义盘符）

## 6. 代码风格与质量要求

## Rust

- `clippy` 视警告为错误（`-D warnings`），不要引入新的 warning。
- 不要 `panic!` 处理可恢复错误；优先 `Result` + 上下文错误信息。
- 关键链路保持“降级但不中断”：例如图谱失败不应拖垮主检索链路。

## UI（React/TypeScript）

- 提交前至少确保可构建：

```bash
npm --prefix ui run build
```

- 保持 IPC 与 HTTP 双运行场景兼容（桌面 / 浏览器）。
- 涉及设置页文案时，记得同步中英 i18n 词条。

## 7. 提交流程（PR）

1. 从 `main` 拉新分支开发（建议语义化分支名）。
2. 代码完成后，至少通过：
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo test --workspace`
   - `npm --prefix ui run build`（若涉及 UI）
3. 提交信息建议使用约定式前缀（示例）：
   - `feat: ...`
   - `fix: ...`
   - `ci: ...`
   - `docs: ...`
4. 提 PR 时请写清楚：
   - 变更动机
   - 主要改动点
   - 手动验证步骤
   - UI 变更截图/GIF（如适用）

## 8. CI 与发布说明

- 常规 CI：`.github/workflows/rust-ci.yml`
  - 在 Ubuntu 跑 `fmt + clippy + test`
- 桌面发布：`.github/workflows/desktop-release.yml`
  - `workflow_dispatch` 或 `v*` tag 触发
  - 构建 Windows/Linux/macOS bundle
  - 自动生成 Draft Release，版本读取 `memori-desktop/tauri.conf.json`

## 9. 安全与数据原则

- 不提交本地模型、数据库、隐私文档或日志。
- 遵守 Local-First：默认不引入云依赖和隐式遥测。
- 若改动会影响数据目录、配置目录或模型请求行为，请在 PR 中明确说明迁移/兼容策略。

## 10. 常见问题（贡献者视角）

- `cargo` 提示找不到 `Cargo.toml`：请先 `cd` 到仓库根目录再执行命令。
- Linux 构建报 `glib/gobject/gio` 缺失：先安装第 3 节中的系统依赖。
- Ollama 404 模型不存在：确认模型 tag（如 `:latest`）与运行时环境变量一致。
- UI 显示“未检测到 Tauri 宿主环境”：你当前是在浏览器模式，或桌面端未启动。

---

如果你准备提交较大改动（例如 IPC 签名、存储 schema、watch 行为），建议先开 issue 或 draft PR 对齐方案，再进入实现。
