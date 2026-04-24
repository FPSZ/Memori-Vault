# Memori-Vault

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

**Local-first verifiable memory engine.**  
Your documents stay on your machine. Every answer tells you exactly where it came from.

[中文文档](./README.zh-CN.md) · [Contributing](./CONTRIBUTING.md) · [Tutorial](./docs/TUTORIAL.md)

---

## Why not a generic RAG?

Most RAG tools give you an answer and a list of "sources."  
Memori-Vault gives you an **auditable evidence chain**: which document, which chunk, which matching terms, and why it was ranked there. If the context is insufficient, it says so—instead of hallucinating.

|  | Typical RAG | Memori-Vault |
|---|---|---|
| **Storage** | Cloud vector DB or remote service | Single SQLite file on your disk |
| **Evidence** | "Sources" list | Document + chunk + hit terms + score contribution |
| **CJK / Chinese** | Often an afterthought | First-class tokenization, query analysis, and ranking |
| **Citation integrity** | Best-effort | Verified: every claim must trace to a indexed chunk |
| **Agent integration** | Custom API wrappers | Native MCP server + standard HTTP API |
| **Deployment** | SaaS or Docker-heavy | Single binary: `cargo run` or Tauri desktop |
| **License** | Often AGPL / proprietary | Apache 2.0 |

---

## What it does

1. **Watches your folder** (Markdown, TXT, PDF, DOCX) and indexes chunks automatically.
2. **Answers questions** using local LLMs (llama.cpp / vLLM / Ollama) with structured citations.
3. **Builds a knowledge graph** in the background—entities, relations, source chunks—without blocking search.
4. **Exposes an MCP server** so Claude, Codex, and other agents can query your vault with structured tools.

---

## Current Status (v0.3.0)

- **Desktop (Tauri)**: fully functional. Search, settings, scope selection, citations, source preview.
- **Server mode**: HTTP API available for browser/local network and private deployment.
- **Retrieval**: citation validity is solid. Document-level Top-1 is improving (see [baseline](./docs/RETRIEVAL_BASELINE.md)).
- **Enterprise**: private-deployment preview with RBAC, audit, and egress policy.

> **Not a 50k-doc enterprise claim yet.** The current baseline is a checked-in regression suite on small corpora. We're iterating on ranking stability before scaling benchmarks.

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/FPSZ/Memori-Vault.git
cd Memori-Vault

# 2. Desktop dev (requires llama.cpp or Ollama running)
pnpm --dir ui install
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop

# 3. Server dev
cargo run -p memori-server
```

Set your model endpoints:
```bash
export MEMORI_CHAT_ENDPOINT=http://localhost:8001      # qwen3:14b
export MEMORI_GRAPH_ENDPOINT=http://localhost:8002     # qwen3:8b
export MEMORI_EMBED_ENDPOINT=http://localhost:8003     # Qwen3-Embedding-4B
```

---

## Architecture

| Crate | Responsibility |
|---|---|
| `memori-vault` | File watcher, debounce, event stream |
| `memori-parser` | Parse & semantic chunk (Markdown, TXT, PDF, DOCX) |
| `memori-storage` | SQLite: vectors, FTS, graph nodes/edges, task queue |
| `memori-core` | Orchestration, retrieval pipeline, indexing worker |
| `memori-desktop` | Tauri commands & desktop lifecycle |
| `memori-server` | Axum HTTP API + MCP endpoint |
| `ui` | React + Vite + Tailwind v4 |

---

## License

Apache License 2.0.
