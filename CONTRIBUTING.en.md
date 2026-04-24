# Contributing to Memori-Vault (English)

Thanks for contributing.

Memori-Vault is a local-first memory system with desktop and server runtimes.
When in doubt, prioritize correctness, observability, and first-answer speed.

中文版本: [CONTRIBUTING.md](./CONTRIBUTING.md)

## 1. Read First

- Product overview: `README.md`
- Chinese overview: `README.zh-CN.md`
- Engineering rules: `AI.md`
- UI baseline: `UI.md`

## 2. Repository Layout

- `memori-vault`: file watch/debounce/event stream
- `memori-parser`: parse/chunk logic
- `memori-storage`: SQLite persistence and retrieval
- `memori-core`: orchestration, search pipeline, indexing worker
- `memori-desktop`: Tauri IPC shell
- `memori-server`: Axum HTTP shell
- `ui`: React + Vite + Tailwind v4

## 3. Prerequisites

- Rust stable (`1.85+` recommended)
- Node.js 20+
- npm
- Ollama (for local model mode)

## 4. Quality Gates (Required)

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm --dir ui run build
```

## 5. Local Development

Desktop mode:

```bash
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
cargo tauri dev -p memori-desktop
```

Browser/server mode:

```bash
cargo run -p memori-server
pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

## 6. Runtime Principles

1. Keep retrieval available even if graph extraction fails.
2. Do not block first answer on full graph completion.
3. Maintain profile isolation for model settings:
- local provider profile and remote provider profile must not overwrite each other.
4. Keep desktop IPC and server HTTP semantics aligned.

## 7. Documentation Rule

If behavior or interface changes, update docs in the same PR:
- `README.md` + `README.zh-CN.md`
- `CONTRIBUTING.md` + `CONTRIBUTING.zh-CN.md`
- `AI.md` (engineering rules)
- `UI.md` (if visual/interaction baseline changed)

## 8. PR Checklist

- Scope and motivation are clear.
- DTO/API/IPC changes are documented.
- Migration impact is described (settings/storage).
- Manual test steps included.
- UI screenshots/GIF provided for UI changes.

## 9. Security & Privacy

- Never commit local databases, model files, private documents, or secrets.
- No hidden telemetry.
- Keep keys/config handling explicit and documented.
