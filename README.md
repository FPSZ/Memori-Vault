# Memori-Vault

English: current page  
中文文档: [README.zh-CN.md](./README.zh-CN.md)  
Contributing: [CONTRIBUTING.md](./CONTRIBUTING.md) | [中文贡献指南](./CONTRIBUTING.zh-CN.md)
Tutorial: [docs/TUTORIAL.md](./docs/TUTORIAL.md) | 中文辅助: [docs/TUTORIAL.zh-CN.md](./docs/TUTORIAL.zh-CN.md)

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

Memori-Vault is a local-first memory engine for personal and team knowledge.
It combines semantic chunking, vector retrieval, and asynchronous Graph-RAG extraction on Ollama + SQLite, while keeping first-answer speed stable under background indexing.

## Highlights (Current)

- Local-first ingestion pipeline (`.md` / `.txt`) with watcher + semantic chunking.
- Hybrid retrieval: vector similarity + graph context.
- Async indexing refactor:
  - fast path for searchable chunks first
  - deferred graph build in background queue
- Indexing strategy controls:
  - `continuous | manual | scheduled`
  - resource budget `low | balanced | fast`
  - pause/resume + trigger reindex
- Settings center (right drawer):
  - UI language and AI answer language (separate)
  - model provider profiles (local Ollama / remote OpenAI-compatible)
  - watch folder switching
  - top-k retrieval control
  - personalization (font, size, theme)
- Search scope selector:
  - nested folder expand/collapse
  - multi-select files/folders
- Source cards:
  - markdown preview for `.md`
  - expand/collapse
  - open file location

## Runtime Modes

1. Desktop mode:
- Tauri shell + UI + IPC backend.

2. Server mode:
- `memori-server` exposes HTTP APIs for local/browser access and private deployment.
- The current product experience is desktop-first; browser-facing UI support is still being aligned with the server runtime.

## Enterprise (Private Deployment v1 Preview)

- Single-tenant private deployment for engineering organizations.
- Preview auth/session entry plus API RBAC (`viewer/user/operator/admin`).
- Admin APIs for health, metrics, policy, audit, reindex, pause/resume.
- Model governance: local-first with remote egress allowlist enforcement.
- Deployment assets included (`deploy/systemd`, env template, backup/restore scripts).

Current note:
- Enterprise deployment is available as a private deployment preview in `v0.2.0`.
- Auth/session flows are suitable for controlled internal environments first and will continue to harden in later releases.

Details: [docs/enterprise.md](./docs/enterprise.md)

## Architecture

Workspace crates:
- `memori-vault`: watch/debounce/event stream
- `memori-parser`: parse/chunk
- `memori-storage`: SQLite + vector/graph/task metadata
- `memori-core`: orchestration, retrieval, indexing worker
- `memori-desktop`: Tauri commands and desktop lifecycle
- `memori-server`: Axum APIs
- `ui`: React + Vite + Tailwind v4 frontend

## Development Quick Start

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
npm --prefix ui run build
```

Desktop dev:

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop
```

Server dev:

```bash
cargo run -p memori-server
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## Notes

- Ollama local runtime is recommended for local provider mode.
- Remote provider mode is optional and user-configured.
- Enterprise policy can enforce `local_only` or remote allowlist mode.
- Legacy theme key `memori-theme-mode` is migration-only; active key is `memori-theme`.

## License

Apache License 2.0.
