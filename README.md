# Memori-Vault

English: current page  
中文文档: [README.zh-CN.md](./README.zh-CN.md)  
Contributing: [CONTRIBUTING.md](./CONTRIBUTING.md) | [中文贡献指南](./CONTRIBUTING.zh-CN.md)

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

Memori-Vault is a local-first memory system for personal knowledge and digital assets.
It runs on-device by default, supports desktop and browser/server modes, and keeps retrieval available even while graph indexing is still in progress.

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

2. Browser mode:
- `memori-server` + UI over HTTP (no Tauri host required).

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

Browser/server dev:

```bash
cargo run -p memori-server
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## Notes

- Ollama local runtime is recommended for local provider mode.
- Remote provider mode is optional and user-configured.
- Legacy theme key `memori-theme-mode` is migration-only; active key is `memori-theme`.

## License

Apache License 2.0.
