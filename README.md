# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

**问你的文档。知道答案从哪来的。**

[English](./README.en.md) · [贡献指南](./CONTRIBUTING.md) · [教程](./docs/TUTORIAL.zh-CN.md)

---

## 解决什么问题？

你的 Markdown 笔记、PDF、DOCX、代码文档散落在各个文件夹里。你想提问并得到**精确、可追溯的回答**——而不是一份胡编乱造的摘要加一串模糊的"参考来源"。

Memori-Vault 是一个**本地优先的记忆引擎**：监听你的文件夹，把所有内容索引进一个 SQLite 文件，然后用**可审计的证据**回答问题——精确到第几段、哪些词匹配、为什么排第一、来源文件是哪个。如果上下文不足，它会明确拒绝，而不是瞎编。

没有云端。没有 Docker。没有向量数据库。就一个二进制文件和你的文档。

---

## 为什么不用其他 RAG 工具？

| 你关心的                       | 典型 RAG               | Memori-Vault                              |
| ------------------------------ | ---------------------- | ----------------------------------------- |
| **我的数据在哪？**       | 云服务或远程向量数据库 | 本地单一 SQLite 文件                      |
| **能离线运行吗？**       | 通常需要联网           | 本地模型完全离线                          |
| **怎么知道答案是真的？** | "来源"列表             | 片段级引用，带命中词和分数追踪            |
| **中文/CJK 文档？**      | 通常是事后补丁         | 原生级分词、查询分析和排序                |
| **Agent 集成**           | 自定义 API 封装        | 原生 MCP 服务器——Claude、Codex 即插即用 |
| **部署**                 | 重型 Docker / SaaS     | 单一二进制：`cargo run` 或 Tauri 桌面端 |
| **协议**                 | AGPL 或闭源            | **Apache 2.0**——商用无限制        |

---

## 核心优势

### 1. 可验证的答案，不是猜测

每个回答包含：

- **哪个文档**、**哪一段**内容支撑了这个说法
- **哪些查询词**匹配上了、在什么位置
- **为什么它排第一**——向量分数、词法分数、短语匹配、路径匹配
- **门控决策**——如果上下文不足，明确拒绝，不 hallucinate

### 2. 原生中文/CJK 支持

从底层为中文文档设计：

- CJK 感知查询解析：自动剥离问题后缀（"是什么"、"怎么"）和处理填充词
- 48 个高频噪声词过滤（"新增"、"功能"、"支持"），减少虚假匹配
- 混合脚本分词："实现 async 方法"这类查询也能正确处理

### 3. 一个二进制，零依赖

- **Rust**——内存安全、高性能、单一静态二进制
- **SQLite**——向量、全文搜索、图谱元数据、任务队列，全在一个文件里
- **Tauri 桌面 + Axum 服务端**——同一套核心代码，桌面或服务端部署
- 没有 Python。没有 Docker。没有 Postgres。没有 Milvus。

### 4. 后台 Graph-RAG，不阻塞搜索

实体和关系抽取在后台队列运行。片段索引完成后搜索立即可用——不需要等图谱构建完成。

### 5. 通过 MCP 接入 Agent

Claude Code、Codex、OpenCode 可通过标准 MCP 工具（`ask`、`search`、`get_source`）查询你的知识库——不是自定义 HTTP 封装。

---

## 快速开始

```bash
# 克隆
git clone https://github.com/FPSZ/Memori-Vault.git
cd Memori-Vault

# 桌面端开发（需先启动本地 LLM 后端：llama.cpp、vLLM 或 Ollama）
pnpm --dir ui install
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop

# 或仅服务端
cargo run -p memori-server
```

配置模型端点：

```bash
export MEMORI_CHAT_ENDPOINT=http://localhost:8001    # 如 qwen3:14b
export MEMORI_GRAPH_ENDPOINT=http://localhost:8002   # 如 qwen3:8b
export MEMORI_EMBED_ENDPOINT=http://localhost:8003   # 如 Qwen3-Embedding-4B
```

---

## 架构

| 模块               | 职责                                         |
| ------------------ | -------------------------------------------- |
| `memori-vault`   | 文件监听、防抖、事件流                       |
| `memori-parser`  | 解析与语义分块（Markdown、TXT、PDF、DOCX）   |
| `memori-storage` | SQLite：密集向量、FTS、图谱节点/边、任务队列 |
| `memori-core`    | 检索链路、索引 worker、引擎编排              |
| `memori-desktop` | Tauri 命令与桌面生命周期                     |
| `memori-server`  | Axum HTTP API + MCP 端点                     |
| `ui`             | React + Vite + Tailwind v4                   |

---

## 当前状态（v0.3.0）

- **桌面端**：功能完整——搜索、引用、来源预览、范围选择、设置。
- **服务端模式**：HTTP API 可用，支持浏览器/局域网接入和私有化部署。
- **检索质量**：引用可靠性已验证。文档级 Top-1 持续优化中（见[基线报告](./docs/RETRIEVAL_BASELINE.md)）。
- **企业版**：私有化部署预览，含 RBAC、审计日志、外连策略。

---

## 许可证

Apache License 2.0.
