# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

**本地优先、可验证的记忆引擎。**  
你的文档留在本地机器上。每个答案都会告诉你它来自哪里。

[English](./README.md) · [贡献指南](./CONTRIBUTING.zh-CN.md) · [教程](./docs/TUTORIAL.zh-CN.md)

---

## 为什么不是普通 RAG？

大多数 RAG 工具给你一个答案和一份"来源"列表。  
Memori-Vault 给你的是**可审计的证据链**：哪个文档、哪个片段、哪些匹配词、以及它为什么排在那里。如果上下文不足，它会明确说——而不是编造。

|  | 典型 RAG | Memori-Vault |
|---|---|---|
| **存储** | 云端向量数据库或远程服务 | 本地单一 SQLite 文件 |
| **证据** | "来源"列表 | 文档 + 片段 + 命中词 + 分数贡献 |
| **中文/CJK** | 通常是事后补丁 | 原生级分词、查询分析和排序 |
| **引用完整性** | 尽力而为 | 已验证：每个说法必须追溯到已索引片段 |
| **Agent 集成** | 自定义 API 封装 | 原生 MCP 服务器 + 标准 HTTP API |
| **部署** | SaaS 或重型 Docker | 单一二进制：`cargo run` 或 Tauri 桌面端 |
| **协议** | 常为 AGPL / 闭源 | Apache 2.0 |

---

## 功能概述

1. **监听文件夹**（Markdown、TXT、PDF、DOCX），自动解析并分块索引。
2. **回答问题**：使用本地大模型（llama.cpp / vLLM / Ollama），附带结构化引用。
3. **后台构建知识图谱**：实体、关系、来源片段——不阻塞搜索。
4. **暴露 MCP 服务器**：Claude、Codex 等 Agent 可通过标准工具查询你的知识库。

---

## 当前状态（v0.3.0）

- **桌面端（Tauri）**：功能完整。搜索、设置、范围选择、引用、来源预览。
- **服务端模式**：HTTP API 可用，支持浏览器/局域网接入和私有化部署。
- **检索质量**：引用可靠性已验证。文档级 Top-1 持续优化中（见[基线报告](./docs/RETRIEVAL_BASELINE.md)）。
- **企业版**：私有化部署预览，含 RBAC、审计、外连策略。

> **尚未承诺 50,000 文档规模的企业级精度。** 当前基线是小样本回归测试集上的结果。我们正持续迭代排序稳定性，再推进大规模基准测试。

---

## 快速开始

```bash
# 1. 克隆
git clone https://github.com/FPSZ/Memori-Vault.git
cd Memori-Vault

# 2. 桌面端开发（需先启动 llama.cpp 或 Ollama）
pnpm --dir ui install
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop

# 3. 服务端开发
cargo run -p memori-server
```

配置模型端点：
```bash
export MEMORI_CHAT_ENDPOINT=http://localhost:8001      # qwen3:14b
export MEMORI_GRAPH_ENDPOINT=http://localhost:8002     # qwen3:8b
export MEMORI_EMBED_ENDPOINT=http://localhost:8003     # Qwen3-Embedding-4B
```

---

## 架构

| 模块 | 职责 |
|---|---|
| `memori-vault` | 文件监听、防抖、事件流 |
| `memori-parser` | 解析与语义分块（Markdown、TXT、PDF、DOCX） |
| `memori-storage` | SQLite：向量、FTS、图谱节点/边、任务队列 |
| `memori-core` | 引擎编排、检索链路、后台索引 worker |
| `memori-desktop` | Tauri 命令与桌面生命周期 |
| `memori-server` | Axum HTTP API + MCP 端点 |
| `ui` | React + Vite + Tailwind v4 |

---

## 许可证

Apache License 2.0.
