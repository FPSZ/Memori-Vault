# Memori-Vault Tutorial (Primary)

This is the primary quick-start and usage tutorial for `v0.2.0`.

Chinese companion: [TUTORIAL.zh-CN.md](./TUTORIAL.zh-CN.md)

## 1. What You Need

- Desktop app (recommended) or `memori-server` mode.
- A folder containing your knowledge files (`.md`, `.txt` currently).
- Model runtime:
  - Local-first: Ollama running locally.
  - Remote: OpenAI-compatible endpoint + API key.

## 2. First Launch (Desktop)

1. Open Memori-Vault.
2. Click **Settings** (top-right gear).
3. If no model is configured yet:
   - The app does **not** auto-start Ollama or a remote runtime.
   - The search box stays disabled.
   - An inline red hint is shown in the search box area: `Model is not configured. Go to Settings > Models.`
4. In **Basic**:
   - Pick your watch folder.
   - Choose retrieval `Top-K` (10 or 20 is a good start).
5. In **Models**:
   - Select provider: **Local Ollama** or **Remote API**.
   - Configure endpoint / key / model roles (`chat`, `graph`, `embed`).
   - Click **Test Connection**.
   - Click **Save Configuration**.

Expected result:
- Status should show reachable.
- Missing-role warning should be empty before production use.
- Once the active provider is valid, the search box becomes editable immediately.

## 3. Local Ollama Setup (Recommended)

Example local defaults:
- `chat_model`: `qwen2.5:7b`
- `graph_model`: `qwen2.5:7b`
- `embed_model`: `nomic-embed-text:latest`
- `endpoint`: `http://localhost:11434`

Verify locally:
```bash
ollama list
```

If a model is missing, use pull from app (when available) or:
```bash
ollama pull qwen2.5:7b
ollama pull nomic-embed-text:latest
```

## 4. Search Workflow

1. Ask a question in the search box.
2. Wait for first answer.
3. Review **Answer**, **Citations**, **Evidence**, and **Retrieval Metrics** together.
4. Use scope selector (left side in search bar) to narrow to selected files/folders.

Notes:
- Graph extraction is asynchronous; early answers may improve over time.
- Retrieval scope affects precision and speed.
- Citations are collapsed by default and can be expanded when you need raw supporting text.
- Evidence cards are grouped by document and deduplicated before display.
- Retrieval metrics show instrumented stage timings plus total/measured/untracked time separation.

## 5. Indexing Modes (Advanced)

You can configure:
- `continuous`: background indexing keeps running (default).
- `manual`: only when manually triggered.
- `scheduled`: runs in configured time window.

Resource budget:
- `low`: lowest background pressure (best for laptop foreground UX).
- `balanced`: normal.
- `fast`: highest throughput.

## 6. Server Mode (Browser Access)

Start backend:
```bash
cargo run -p memori-server
```

Start UI:
```bash
npm --prefix ui run dev -- --host 127.0.0.1 --port 1420 --strictPort
```

For enterprise/private deployment details:
- [enterprise.md](./enterprise.md)
- [enterprise.zh-CN.md](./enterprise.zh-CN.md)

## 7. Troubleshooting

### Connection fails but models look configured
- Check endpoint format (`http://localhost:11434` for local).
- For remote, ensure endpoint has correct base path and valid key.
- Remote provider still requires all three roles (`chat`, `graph`, `embed`) to be configured.
- Re-run **Test Connection** after switching provider.

### App opens but search is disabled
- This is expected when no valid active provider is configured yet.
- Go to **Settings > Models**, choose **Local Ollama** or **Remote API**, then save a complete profile.
- The app no longer auto-falls back to local Ollama on first launch.

### Vault stats stay at `0`
- Confirm watch folder exists and contains supported files.
- Confirm indexing mode is not paused/manual-without-trigger.
- Trigger reindex from **Advanced** tab.

### Window opens too small or wrong position
- Newer versions sanitize invalid persisted window states.
- If needed, remove bad window fields in app settings file and relaunch.

### Markdown table looks broken
- Usually caused by chunk boundaries splitting table syntax.
- Keep related table sections in the same note block when possible.
- Prefer narrower scope to reduce mixed fragment context.

## 8. Release Readiness Checklist

Before publishing:
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `npm --prefix ui run build`
- Verify version consistency (`workspace`, `tauri.conf.json`, `ui/package.json`).
- Prepare release notes in `docs/RELEASE_NOTES_v0.2.0.md`.

## 9. Optional Smoke Scripts

For local/manual validation:

- Desktop/server smoke bootstrap:
```powershell
.\scripts\smoke-start.ps1
```

- Stop smoke services:
```powershell
.\scripts\smoke-stop.ps1
```

- External corpus usability smoke:
```powershell
.\scripts\test-usability-smoke.ps1 -CorpusRoot <your-corpus-dir>
```

Notes:
- These scripts are helper entry points, not the product's runtime contract.
- `smoke-start.ps1` can skip local model validation when you are testing UI/server flows separately.
