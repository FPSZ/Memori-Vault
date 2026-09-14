# Memori-Vault 1.5.2 Release Notes

## Summary

`1.5.2` is the engineering-hardening and **OCR** release for Memori-Vault. It consolidates the `dev` line work after `1.5.0` — security/productization hardening (rate limiting, request-id tracing, OpenAPI 3.1, session lifecycle, OS keychain for API keys), a cross-platform CI matrix with dependency scanning, a 50k-scale performance harness, and an answer-layer LLM-judge — and adds OCR ingestion for images and scanned documents.

The product boundary is unchanged: Memori-Vault remains a local-first, verifiable **Memory OS Lite**. Document answers must still be backed by chunk-level citations, and conversation/project memory is surfaced separately as `memory_context` — it can never masquerade as a document citation.

## Highlights

- **OCR ingestion (new)**: standalone images (`png`/`jpg`/`jpeg`), text-layer-free scanned PDFs, and DOCX embedded images are OCR'd with tesseract (`chi_sim`) **at index time**; recognized text joins the normal chunk/retrieval pipeline.
- **OCR stays out of the answer path**: ask-time reference excerpts and desktop file preview never run OCR, so a scanned document can no longer stall the retrieval/answer chain.
- **Configurable OCR engine**: `MEMORI_OCR_TESSERACT_PATH` > `settings.json` → `ocr_tesseract_path` > PATH auto-detection. Desktop exposes it under Settings → Models; the server exposes `POST /api/settings/ocr-path` (operator role). Both take effect without restarting; already-indexed files need a re-index.
- **Security hardening**: API keys move to the OS keychain (settings.json keeps only a sentinel), per-IP rate limiting with request-id/trace across the retrieval chain, CORS methods/headers allowlisted, explicit logout endpoint + active-session cap (2048), and audit-write failures escalated from warn to error.
- **OpenAPI 3.1 + Swagger UI**: table-driven spec served from `/api/openapi.json` with a drift self-check, plus an offline Swagger UI page for interactive API docs.
- **CI**: cross-platform matrix (ubuntu/windows/macos), `cargo-deny` + `pnpm audit` scanning, an offline deterministic retrieval quality gate, and a reusable 50k perf-scale workflow.
- **Scale**: 50k-document benchmark harness (sequential/concurrent P50/P95/P99) plus the storage connection-model change (single connection → WAL + read-only pool; contention 6.26× → 1.72×, concurrent throughput 5.4×).
- **Evaluation**: v2 hard benchmark (548 documents / 126 cases) and the first answer-layer LLM-judge baseline (126 cases, correct/partial/incorrect).

## OCR (Images And Scanned Documents)

OCR runs **only at index time**, and only for formats it can actually read:

| Source | Support |
| --- | --- |
| Standalone images (`png` / `jpg` / `jpeg`) | Yes |
| Scanned PDFs **without** a text layer | Yes (page XObject images) |
| DOCX embedded images (`word/media/*`) | Yes |
| PDFs that already have a text layer | Text layer is used as-is |

Extraction is deliberately conservative — the evidence chain is the product's core value, so unsupported or suspicious input is skipped rather than indexed as noise:

- only 8 bit/component images are accepted (higher bit depths would be reinterpreted as garbage pixels);
- `ImageMask` stencils and unsupported color spaces (indexed / separation / CMYK) are skipped, while `DeviceGray` / `DeviceRGB` and `ICCBased` profiles (including indirect references) are supported;
- decompression is bounded (`take(limit + 1)`) so a crafted flate stream cannot exhaust memory, and the decoded pixel budget is capped per image;
- the filter chain is parsed strictly: an unparseable `/Filter` skips the image instead of silently degrading to "no filter" (which would decode compressed bytes as raw pixels).

**Measured behaviour** (repo scan fixture `Memory_Test_V2/special_005_扫描件_苍岭_对账.pdf`, tesseract + `chi_sim`): one page decodes to one image in ~0.8s and yields readable Chinese text, e.g. `…项目的对账窗口为每月 8 号…`. However **entity names can be misread** (`苍岭` → `苑岭`/`苔岭`), so OCR text should be treated as recall support, not as an exact-match or exact-quote source.

## API And Observability

- `POST /api/settings/ocr-path` — set/clear the OCR executable path (operator).
- OpenAPI 3.1 contract at `/api/openapi.json`; a route-table consistency test fails the build if a route is registered without a spec entry.
- Swagger UI available at `/api/docs` (offline assets, no CDN dependency).
- Every request carries a request-id that is propagated into the retrieval chain and audit records.

## Engineering Hardening

- Duplicate model helpers merged into a single source of truth; Markdown plugin typing tightened (no `any`/`unknown[]`).
- Top-level React ErrorBoundary so render failures degrade to an error page instead of a blank screen.
- CI upgraded to `cargo clippy --workspace --all-targets -D warnings` plus HTTP end-to-end integration tests.
- README maturity badges split into ✅ verified / 🚧 in progress / 📐 designed.
- Handoff documentation and AI branch guardrails (`AGENTS.md`, `CLAUDE.md`) so all collaboration work stays on the `collab` branch.

## Fixes

- OCR: tesseract stdout pipe deadlock (dense pages always timed out and burned 30s) — stdout/stderr are now drained by dedicated threads with a timeout watchdog.
- OCR: DOCX embedded-image temp directory was never created on a clean machine (silent failures).
- OCR: images were read as UTF-8 text by the indexing entry point, so OCR never ran and every image wrote an `indexing_runtime.last_error`.
- OCR: no-text-layer PDFs returned `None` instead of an empty string, producing a misleading "file read failed (possibly locked)" error.
- OCR: `/Filter [/ASCII85Decode /DCTDecode]` (ASCII85-wrapped JPEG) images were skipped entirely.
- OCR: tesseract path changes required an application restart; the probing cache is now keyed by the configured value.
- Perf harness: `--start-doc` resume no longer double-counts chunks (the report now uses the actual chunk count in the DB).
- Perf CI: the report artifact is uploaded with `if: always()`, so it survives a failed contention assertion.
- Server mode applies `index_filter` (aligned with desktop); `/v1` URL building no longer double-prefixes.

## Known Boundaries

- OCR requires tesseract with the `chi_sim` language pack installed (or a configured path); without it, images/scans are indexed as empty and OCR is simply skipped.
- Mixed PDFs (a text layer plus scanned pages) do not OCR the scanned pages; `ppt`/`xlsx` embedded images are not covered; `CCITTFaxDecode` (G4 fax compression) and `JPXDecode` (JPEG 2000) are not supported.
- The 50k-scale numbers are harness results, not a claim of production-scale accuracy; the v2 hard-benchmark numbers are a measured baseline, not a precision guarantee.
- Refusal behaviour and gating false negatives are still being tuned (Q1/Q2 quality round).
- Changing the OCR path does not retroactively re-index already-indexed files; trigger a re-index to apply it.

## Upgrade Notes

- Version is `1.5.2` across the Cargo workspace, the UI package, and the Tauri desktop config.
- Existing local SQLite data stays local; no migration is required for OCR (it only adds newly extractable content on the next index pass).
- To enable OCR, install tesseract (+ `chi_sim`) or point `ocr_tesseract_path` (desktop Settings → Models, server `POST /api/settings/ocr-path`, or the `MEMORI_OCR_TESSERACT_PATH` environment variable) and then trigger a re-index.
