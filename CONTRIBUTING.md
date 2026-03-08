# Contributing to Memori-Vault (English)

Thanks for contributing to Memori-Vault.

Memori-Vault is a local-first memory system. The core goal is to complete file watching, parsing, embedding, graph extraction, and retrieval on-device, without cloud dependency.

中文版本: [CONTRIBUTING.zh-CN.md](./CONTRIBUTING.zh-CN.md)

## 1. Read First

- Project overview (English): `README.md`
- Project overview (Chinese): `README.zh-CN.md`
- License: `LICENSE` (Apache-2.0)

## 2. Repository Layout

Current Rust workspace members (root `Cargo.toml`):

- `memori-vault`: file watching, async debounce, bounded backpressure channel (`.md/.txt`)
- `memori-parser`: text normalization and semantic chunking
- `memori-storage`: SQLite vector/graph persistence and retrieval
- `memori-core`: engine orchestration, daemon lifecycle, Ollama integration
- `memori-desktop`: Tauri v2 desktop shell and IPC commands
- `memori-server`: Axum HTTP server (browser mode / no desktop shell)
- `ui`: React + Vite + Tailwind v4 frontend
- `scripts`: local smoke scripts (Windows PowerShell)

## 3. Prerequisites

Required:

- Rust stable (recommended `1.85+`)
- Node.js 20+
- npm
- Ollama (local model runtime)

Linux dependencies for Tauri build (aligned with CI, Ubuntu):

- `pkg-config`
- `patchelf`
- `libglib2.0-dev`
- `libgirepository1.0-dev`
- `libgtk-3-dev`
- `libayatana-appindicator3-dev`
- `librsvg2-dev`
- `libsoup-3.0-dev`
- `libwebkit2gtk-4.1-dev` (or `4.0-dev`, distro-dependent)

## 4. Local Development Flows

### A. Rust quality gates (minimum before PR)

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

### B. Desktop mode (Tauri + UI)

1. Install frontend dependencies:

```bash
npm ci --prefix ui
```

2. Start UI dev server (fixed port `1420`):

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

3. Start desktop shell (run in repo root):

```bash
cargo tauri dev -p memori-desktop
```

If `tauri-cli` is not installed:

```bash
cargo run -p memori-desktop
```

### C. Browser mode (without Tauri host)

1. Start HTTP backend:

```bash
cargo run -p memori-server
```

2. Start UI:

```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

3. Open: `http://127.0.0.1:1420`

### D. Windows one-command smoke

```powershell
pwsh -ExecutionPolicy Bypass -File .\scripts\smoke-start.ps1
pwsh -ExecutionPolicy Bypass -File .\scripts\smoke-stop.ps1
```

## 5. Common Environment Variables

- `MEMORI_WATCH_ROOT`: watch directory
- `MEMORI_DB_PATH`: SQLite path (default `.memori.db` in current directory)
- `MEMORI_EMBED_MODEL`: embedding model (default `nomic-embed-text:latest`)
- `MEMORI_CHAT_MODEL`: answer model (default `qwen2.5:7b`)
- `MEMORI_GRAPH_MODEL`: graph extraction model (default `qwen2.5:7b`)
- `MEMORI_OLLAMA_BASE_URL`: embedding base URL (default `http://localhost:11434`)
- `MEMORI_OLLAMA_CHAT_ENDPOINT`: chat endpoint (default `http://localhost:11434/api/chat`)
- `MEMORI_SERVER_ADDR`: `memori-server` bind address (default `127.0.0.1:3757`)
- `OLLAMA_MODELS`: Ollama model directory (common on Windows custom drives)

## 6. Code Quality Expectations

### Rust

- `clippy` warnings are treated as errors (`-D warnings`).
- Avoid `panic!` for recoverable paths; prefer `Result` with clear context.
- Keep graceful degradation on critical paths (e.g. graph extraction failure must not break main retrieval flow).

### UI (React/TypeScript)

Before submitting UI changes:

```bash
npm --prefix ui run build
```

- Keep both IPC and HTTP runtime paths compatible (desktop and browser).
- When changing settings copy, update both Chinese and English i18n entries.

## 7. Pull Request Workflow

1. Create a branch from `main`.
2. Before opening PR, pass at least:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo test --workspace`
   - `npm --prefix ui run build` (if UI changed)
3. Prefer conventional commit prefixes:
   - `feat: ...`
   - `fix: ...`
   - `ci: ...`
   - `docs: ...`
4. PR description should include:
   - why this change is needed
   - key implementation points
   - manual verification steps
   - screenshots/GIF for UI changes (if applicable)

## 8. CI and Release

- Main CI: `.github/workflows/rust-ci.yml`
  - Runs `fmt + clippy + test` on Ubuntu
- Desktop release pipeline: `.github/workflows/desktop-release.yml`
  - Triggered by `workflow_dispatch` or `v*` tags
  - Builds Windows/Linux/macOS bundles
  - Creates draft GitHub Release
  - Version is read from `memori-desktop/tauri.conf.json`

## 9. Security and Data Principles

- Do not commit local models, databases, private documents, or logs.
- Follow local-first principle: no hidden telemetry and no unnecessary cloud dependency.
- If your change impacts config paths, data paths, or model routing, document migration/compatibility behavior in the PR.

## 10. Contributor FAQ

- `cargo` cannot find `Cargo.toml`: run commands from repository root.
- Linux build fails on `glib/gobject/gio`: install system dependencies from section 3.
- Ollama `404 model not found`: verify model tags (for example `:latest`) and runtime env vars.
- UI shows "Tauri host not detected": you're in browser mode or desktop shell is not running.

---

For large changes (IPC contracts, storage schema, watcher behavior), open an issue or draft PR first to align design before implementation.
