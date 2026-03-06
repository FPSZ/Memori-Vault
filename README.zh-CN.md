# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

中文文档：当前页  
English: [README.md](./README.md)

## 项目简介

Memori-Vault 是一个 **本地优先（Local-First）** 的个人记忆系统：
- 监听本地文档变化并自动解析；
- 使用本地模型进行向量化与关系抽取；
- 将向量和图谱写入本地存储，支持检索与后续推理。

它不依赖云端，不做隐式遥测，目标是在 2026 年提供可长期演化的个人数据主权基础设施。

## 愿景

Memori-Vault 的目标是构建一个真正可控的个人知识底座：
- **Local-First**：摄入、索引、检索、推理尽可能都在本机完成；
- **Zero-Config**：默认可用，降低对模型参数和存储细节的理解门槛；
- **Graph-RAG Native**：向量相似度与实体关系图协同工作。

## 架构

项目按 Rust crate 进行物理隔离：

- `memori-vault`：文件监听、防抖与背压事件投递。
- `memori-core`：守护进程生命周期、任务编排、调用链路。
- `memori-parser`：文本分块与结构化预处理。
- `memori-storage`：向量与图谱持久化存储。

当前主流程：

`memori-vault -> memori-core -> memori-parser -> memori-storage`

## 当前状态

当前阶段：**Phase 2（Headless Backend）**。

已完成：
- 实时监听 + 异步防抖 + 背压通道；
- 文本分块、向量化、SQLite 持久化；
- 实体关系抽取与图谱表落盘；
- CLI 交互检索与基础 CI 流程。

未完成：
- Tauri GUI 可视化层；
- 图谱可视化与交互式关系探索页面。

## 路线图

### Phase 1 - 摄入与向量化核心
- 文件监听与稳定摄入链路；
- 语义分块与向量检索闭环。

### Phase 2 - Graph-RAG 与长期记忆
- 实体/关系抽取；
- 本地图谱持久化；
- 混合检索（向量 + 图路径）。

### Phase 3 - Tauri 桌面体验
- IPC 接入核心引擎；
- 可视化检索界面与来源追踪；
- 图谱关系浏览与交互。

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## 许可证

Apache License 2.0
