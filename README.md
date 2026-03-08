# Memori-Vault

English: current page  
中文文档: [README.zh-CN.md](./README.zh-CN.md)
Contributing: [CONTRIBUTING.md](./CONTRIBUTING.md) | [中文贡献指南](./CONTRIBUTING.zh-CN.md)

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-111111?style=flat-square)](./LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/Rust-1.85%2B-111111?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![CI](https://img.shields.io/github/actions/workflow/status/FPSZ/Memori-Vault/rust-ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/FPSZ/Memori-Vault/actions/workflows/rust-ci.yml)

Memori-Vault is a local-first memory system for personal knowledge and digital assets.
It is designed to run entirely on-device, with no cloud dependency, no silent telemetry,
and no compromise on data ownership.

## Intro

Memori-Vault is a local-first memory engine for personal knowledge.
It watches local files, chunks content, generates embeddings, extracts graph relations,
and persists everything to local storage for retrieval and reasoning.

## Vision

In 2026, personal data sovereignty is not optional.
Memori-Vault is built as a practical answer:

- Local-First: ingestion, indexing, retrieval, and generation happen on your machine.
- Zero-Config: users do not need to understand vector dimensions, model routing, or storage internals.
- Graph-RAG Native: semantic similarity and explicit relationships work together for long-term memory.

This project is not a chat wrapper. It is an operating layer for private, durable, personal intelligence.

## Architecture

Memori-Vault keeps core concerns physically isolated by crate boundaries:

- `memori-vault` (watch layer): file-system event capture, debounce, and backpressure-safe streaming.
- `memori-core` (orchestration layer): daemon lifecycle, event routing, and pipeline coordination.
- `memori-parser` (understanding layer): text normalization and semantic chunk generation.
- `memori-storage` (persistence layer): local metadata/graph/vector persistence abstraction.

Pipeline (current headless design):

`memori-vault -> memori-core -> memori-parser -> memori-storage`

## Status

Current stage: **Phase 2 - Headless Backend Development**.

Implemented:

- Rust workspace with crate-level boundaries.
- Real-time file watching with debounce and bounded-channel backpressure.
- Parser, embedding, and SQLite persistence pipeline.
- Graph extraction and graph persistence tables.
- CLI retrieval flow and CI baseline (format, lint, tests).

Not yet released:

- GUI / Tauri desktop shell.
- End-user application packaging.

## Roadmap

### Phase 1 - Ingestion & Vectorization Core (Headless)

- File watch pipeline for `.md` / `.txt`.
- Semantic chunking with overlap.
- Local embedding bridge and Top-K retrieval in CLI flow.

### Phase 2 - Graph-RAG & Long-Term Memory

- Local entity/relation extraction during ingestion.
- SQLite graph persistence for nodes and edges.
- Hybrid retrieval: vector similarity + multi-hop graph traversal.

### Phase 3 - Tauri IPC & Desktop UX

- Core engine exposure through Tauri commands.
- Fast summon window and focused interaction loop.
- Streaming answer UI with source traceability.

## Principles

- No cloud dependency for core capabilities.
- No uncontrolled memory growth during ingestion.
- No unchecked errors in critical paths.
- No UI-first shortcuts before core correctness.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

## License

Apache License 2.0.
