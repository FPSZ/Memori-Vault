# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

中文文档：当前页  
English: [README.md](./README.md)  
贡献指南：[CONTRIBUTING.zh-CN.md](./CONTRIBUTING.zh-CN.md) | English: [CONTRIBUTING.md](./CONTRIBUTING.md)

## 项目简介

Memori-Vault 是一个本地优先（Local-First）的个人记忆系统，支持桌面版与浏览器/服务端模式。
核心目标是让“首问尽快返回”，并把图谱补全放到后台异步进行。

## 当前能力

- 本地文档摄入：监听 `.md/.txt`，自动解析与分块。
- 混合检索：向量召回 + 图谱上下文。
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

## 运行模式

1. 桌面模式（Tauri）：
- UI + IPC + 本地桌面壳。

2. 浏览器模式（Server）：
- `memori-server` 提供 HTTP API，前端通过浏览器访问。

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

浏览器/服务端开发：

```bash
cargo run -p memori-server
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## 说明

- 本地模式建议先启动 Ollama。
- 远程模式为可选项，由用户自行配置 endpoint/key/model。
- 主题旧键 `memori-theme-mode` 仅用于迁移读取，当前使用 `memori-theme`。

## 许可证

Apache License 2.0。
