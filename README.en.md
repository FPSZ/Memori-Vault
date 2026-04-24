# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

**Ask your documents. Know exactly where the answer came from.**

[中文](./README.md) · [Contributing](./CONTRIBUTING.en.md) · [Tutorial](./docs/TUTORIAL.md)

---

## What problem does it solve?

You have thousands of Markdown notes, PDFs, DOCX files, and code docs scattered across folders. You want to ask questions and get **precise, traceable answers** — not a hallucinated summary with a vague "Sources" list.

Memori-Vault is a **local-first memory engine** that watches your folders, indexes everything into a single SQLite file, and answers questions with **auditable evidence**: exact chunk, matching terms, score contribution, and source file. If the context is insufficient, it says so — instead of making things up.

No cloud. No Docker. No vector database. Just a single binary and your files.

---

## Why not Anything-LLM / Quivr / other RAG tools?

| What you care about                         | Typical RAG                       | Memori-Vault                                               |
| ------------------------------------------- | --------------------------------- | ---------------------------------------------------------- |
| **Where is my data?**                 | Cloud service or remote vector DB | Single SQLite file on your disk                            |
| **Can I run it offline?**             | Usually needs internet            | Fully offline with local models                            |
| **How do I know the answer is true?** | "Sources" list                    | Chunk-level citation with hit terms and score trace        |
| **Chinese / CJK documents?**          | Often an afterthought             | Native tokenization, query analysis, and ranking           |
| **Agent integration**                 | Custom API wrappers               | Native MCP server — Claude, Codex, OpenCode plug-and-play |
| **Deployment**                        | Docker-heavy SaaS                 | Single binary:`cargo run` or Tauri desktop app           |
| **License**                           | AGPL or proprietary               | **Apache 2.0** — commercial use, no restrictions    |

---

## Core strengths

### 1. Verifiable answers, not guesses

Every answer includes:

- **Which document** and **which chunk** the claim comes from
- **Which query terms** matched and where
- **Why it ranked first** — dense score, lexical score, phrase match, path match
- **Gating decision** — if context is insufficient, it refuses instead of hallucinating

### 2. Native Chinese / CJK support

Built from the ground up for Chinese documents:

- CJK-aware query parsing with suffix stripping ("是什么", "怎么") and filler handling
- 48-term high-frequency noise filtering ("新增", "功能", "支持") to reduce false matches
- Mixed-script segmentation for queries like "实现 async 方法"

### 3. One binary, zero dependencies

- **Rust** — memory-safe, fast, single static binary
- **SQLite** — vectors, full-text search, graph metadata, and task queue in one file
- **Tauri desktop + Axum server** — same core code, desktop or server deployment
- No Python. No Docker. No Postgres. No Milvus.

### 4. Background Graph-RAG without blocking search

Entity and relation extraction runs in a background queue. Search works immediately after chunk indexing — you don't wait for graph completion.

### 5. Agent-ready via MCP

Claude Code, Codex, and OpenCode can query your vault through standard MCP tools (`ask`, `search`, `get_source`) — not custom HTTP wrappers.

---

## Quick start

```bash
# Clone
git clone https://github.com/FPSZ/Memori-Vault.git
cd Memori-Vault

# Desktop dev (needs a local LLM backend: llama.cpp, vLLM, or Ollama)
pnpm --dir ui install
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop

# Or server only
cargo run -p memori-server
```

Configure your model endpoints:

```bash
export MEMORI_CHAT_ENDPOINT=http://localhost:8001    # e.g. qwen3:14b
export MEMORI_GRAPH_ENDPOINT=http://localhost:8002   # e.g. qwen3:8b
export MEMORI_EMBED_ENDPOINT=http://localhost:8003   # e.g. Qwen3-Embedding-4B
```

---

## Architecture

| Crate              | What it does                                              |
| ------------------ | --------------------------------------------------------- |
| `memori-vault`   | File watcher, debounce, event stream                      |
| `memori-parser`  | Parse and semantically chunk Markdown, TXT, PDF, DOCX     |
| `memori-storage` | SQLite: dense vectors, FTS, graph nodes/edges, task queue |
| `memori-core`    | Retrieval pipeline, indexing worker, orchestration        |
| `memori-desktop` | Tauri commands and desktop lifecycle                      |
| `memori-server`  | Axum HTTP API + MCP endpoint                              |
| `ui`             | React + Vite + Tailwind v4                                |

---

## Current status (v0.3.0)

- **Desktop**: fully functional — search, citations, source preview, scope selection, settings.
- **Server mode**: HTTP API for browser/local network and private deployment.
- **Retrieval**: citation validity is solid. Document-level Top-1 is improving ([baseline](./docs/RETRIEVAL_BASELINE.md)).
- **Enterprise**: private-deployment preview with RBAC, audit logs, and egress policy.

---

## License

Apache License 2.0.
