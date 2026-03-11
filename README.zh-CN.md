# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

中文文档：当前页  
English: [README.md](./README.md)  
贡献指南：[CONTRIBUTING.zh-CN.md](./CONTRIBUTING.zh-CN.md) | English: [CONTRIBUTING.md](./CONTRIBUTING.md)
教程（英文主）：[docs/TUTORIAL.md](./docs/TUTORIAL.md) | 中文辅助：[docs/TUTORIAL.zh-CN.md](./docs/TUTORIAL.zh-CN.md)

## 项目简介

Memori-Vault 是一个本地优先（Local-First）的记忆引擎，面向个人与团队知识场景。
它集成语义分块、向量检索与异步 Graph-RAG 抽取（Ollama + SQLite），并优先保证首问响应速度。

## 当前能力

- 本地文档摄入：监听 `.md/.txt`，自动解析与分块。
- 结构化检索链路：document routing + chunk retrieval + citations/evidence 输出。
- 异步索引重构：
  - 快路径：先向量化并可检索
  - 慢路径：图谱任务入队后台补齐
- 索引策略可配置：
  - 模式：`continuous | manual | scheduled`
  - 资源档位：`low | balanced | fast`
  - 支持暂停/恢复与手动重建
- 设置中心（右侧抽屉）：
  - UI 语言与 AI 回答语言分离
  - 模型来源（本地 Ollama / 远程 OpenAI-compatible）
  - 读取目录切换
  - Top-K 检索条数
  - 个性化（字体、字号、深浅主题）
- 检索范围选择：
  - 支持多选文件/目录
  - 支持子目录分层展开
- 来源卡片增强：
  - Markdown 预览
  - 展开/折叠
  - 打开文件位置

## 当前验证状态

- 本地优先运行时和企业策略收口已经落地。
- 当前 checked-in 离线回归里，citation validity 仍然可靠。
- 但检索精度，尤其是 mixed corpus 的文档级 Top-1，还没有达到可以放心对外承诺的程度。
  - `core_docs` 离线基线：`6` 份文档上 `Top-1=0.6970`
  - `repo_mixed` 离线基线：`11` 份文档上 `Top-1=0.4773`
- 这些数字只是当前仓库小样本回归结果，不是 50,000 文档规模的验证结论。
- 当前机器上的 `live_embedding` 仍被本地 Ollama / embedding 可用性阻塞。

当前口径：

- docs-only 检索可以作为内部基线继续推进
- mixed corpus 检索仍应按 beta / 内部验证口径描述，不能写成高精度已完成

详细基线见：[`docs/RETRIEVAL_BASELINE.md`](./docs/RETRIEVAL_BASELINE.md)

## 运行模式

1. 桌面模式（Tauri）：
- UI + IPC + 本地桌面壳。

2. 服务端模式（Server）：
- `memori-server` 提供 HTTP API，可用于本地/浏览器接入与私有化部署。
- 当前产品体验仍以桌面版为主，面向浏览器的 UI 闭环仍在持续对齐中。

## 企业化能力（私有化 v1 预览）

- 面向研发组织的单租户私有化部署形态。
- 预览阶段认证/会话入口 + API 级 RBAC（`viewer/user/operator/admin`）。
- 管理接口覆盖：健康检查、指标、策略、审计、重建、暂停/恢复。
- 模型治理：本地优先，远程外连由白名单策略控制。
- 附带部署资产（`deploy/systemd`、env 模板、备份/恢复脚本）。

当前说明：
- `v0.2.0` 的企业能力以私有化预览形态提供。
- 认证与会话链路当前更适合受控内部环境，后续版本会继续补强安全细节。

详细说明：[`docs/enterprise.zh-CN.md`](./docs/enterprise.zh-CN.md)

## 架构

工作区模块：
- `memori-vault`：监听、防抖、事件通道
- `memori-parser`：解析与分块
- `memori-storage`：SQLite、向量/图谱/任务元数据
- `memori-core`：引擎编排、检索链路、后台索引 worker
- `memori-desktop`：Tauri 命令与桌面生命周期
- `memori-server`：Axum HTTP 接口
- `ui`：React + Vite + Tailwind v4 前端

## 开发快速开始

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
npm --prefix ui run build
```

桌面开发：

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop
```

服务端开发：

```bash
cargo run -p memori-server
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## 说明

- 本地模式建议先启动 Ollama。
- 远程模式为可选项，由用户自行配置 endpoint/key/model。
- 企业策略支持 `local_only` 与 allowlist 外连模式。
- 主题旧键 `memori-theme-mode` 仅用于迁移读取，当前使用 `memori-theme`。

## 许可证

Apache License 2.0。
