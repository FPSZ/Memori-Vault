# Memori-Vault 1.5.0 Release Notes

## Summary

`1.5.0` is the retrieval-quality and local-model runtime release for Memori-Vault. It consolidates the work from the `dev` line after `1.0.2`: local-first model policy hardening, four-role llama.cpp runtime support, rerank integration, a real Chinese Memory_Test regression suite, visual regression reporting, and the first measured 100-case live retrieval baseline for the rebuilt retrieval pipeline.

This release keeps the product boundary unchanged: Memori-Vault is still a local-first, verifiable knowledge retrieval and long-term memory system. The major change is that retrieval quality is now measured with a realistic internal corpus instead of mostly project-development documents.

## Highlights

- Added a four-role local model runtime: chat, graph, embedding, and rerank use separate llama.cpp endpoints by default (`18001 / 18002 / 18003 / 18004`).
- Integrated chunk-level rerank into the retrieval path with safe fallback: if rerank is disabled or unavailable, retrieval degrades to the existing RRF order instead of failing.
- Rebuilt the retrieval regression harness so it can measure semantic recall, rerank application, chunk Top-1, chunk MRR, service health, rerank health, and gating reasons.
- Replaced the old regression suite with a 100-case Chinese-focused Memory_Test suite covering direct fact lookup, paraphrase recall, anti-common-answer questions, ID/alias retrieval, cross-document synthesis, similar-code disambiguation, multiple file formats, document-type targeting, typo/colloquial robustness, long-condition queries, and refusal behavior.
- Added a visual regression lab in the desktop UI for running, monitoring, and reviewing retrieval tests without relying on mock data.
- Added persistent metrics export to Excel and a self-contained HTML capability benchmark for cross-run comparison.
- Added report version metadata so each visual test report records report schema version, app version, and suite version.
- Hardened enterprise/local-only policy handling so remote model usage is blocked by policy instead of silently replacing local runtime behavior.
- Fixed reference excerpt generation for `.md`, `.markdown`, and `.txt` so normal text files no longer produce misleading binary-document extraction warnings during answer generation.

## Retrieval Metrics

Latest measured run:

- Report: `target/retrieval-regression/live_embedding-full_live-1780648792/report.json`
- Mode/profile: `live_embedding + full_live`
- Suite: `docs/qa/retrieval_regression_suite.json`
- Suite version: `2`
- Report schema version: `1.1`
- Cases: `100`
- Answer/refuse split: `88 / 12`
- Timeouts: `0`
- Service health: `ready`
- Rerank health: `ready`
- Index preparation: `80,251 ms`
- Indexed corpus: `100` documents / `801` chunks

Measured results:

| Metric | Value |
| --- | ---: |
| Overall pass rate | `93/100` |
| Top-1 document hit | `87.50%` |
| Top-3 document recall | `93.18%` |
| Top-1 chunk hit | `81.82%` |
| Top-5 chunk recall | `93.18%` |
| Chunk MRR | `0.8561` |
| Citation validity | `100.00%` |
| Refuse correctness | `91.00%` |
| Rerank applied | `95.00%` |

Compared with the earlier documented mixed-corpus baseline (`repo_mixed`: Top-1 document `47.73%`, Top-3 document `47.73%`, Top-5 chunk `54.55%`, citation validity `100%`, refuse correctness `94%`), this release substantially improves document and chunk retrieval quality on the new live Memory_Test suite while keeping citation validity stable. Refusal behavior remains good but not perfect; the latest live suite still has residual failures and should be treated as a measured beta-quality retrieval baseline, not a claim of broad production-scale accuracy.

Gating reasons observed in the latest 100-case run:

| Gating reason | Count |
| --- | ---: |
| `rerank_confident_release` | `35` |
| `docs_family_multi_chunk_release` | `24` |
| `identifier_grounded_release` | `11` |
| `compound_evidence_release` | `11` |
| `entity_not_grounded` | `7` |
| `score_below_threshold` | `7` |
| `intent_blocked` | `5` |

## Retrieval And Rerank

The retrieval path is now easier to evaluate and diagnose:

- Document routing and chunk retrieval are reported separately.
- Rerank status is recorded as `ready`, `disabled`, or `unavailable`.
- Each case records whether rerank was actually applied.
- Gating decisions are visible in JSON and Markdown reports.
- Chunk-level Top-1 and MRR make rerank improvements measurable.
- Reports include `report_schema_version`, `app_version`, and `suite_version` for future comparison.

The rerank implementation is deliberately defensive. It improves ordering when the local rerank service is healthy, but it does not become a hard dependency for basic retrieval.

## Visual Regression Lab

The desktop regression panel now supports real retrieval test runs instead of mock-only display:

- Start `offline_deterministic` or `live_embedding` runs from the UI.
- Choose `core_docs`, `repo_mixed`, or `full_live` profile.
- Watch live progress, active case, phase, pass/fail count, and run output.
- Browse historical reports from `target/retrieval-regression`.
- Inspect per-case ranks, citation validity, refusal correctness, gating reason, timings, and failure reasons.
- See version metadata in report list and overview so large tests can be compared by app/suite/report version.

## Model Runtime

The local runtime now treats model roles independently:

| Role | Default endpoint | Notes |
| --- | --- | --- |
| Chat | `http://localhost:18001` | Main answer generation model |
| Graph | `http://localhost:18002` | Entity/graph extraction model |
| Embedding | `http://localhost:18003` | Embedding endpoint, requires embedding mode |
| Rerank | `http://localhost:18004` | Rerank endpoint, requires reranking mode |

The desktop settings and runtime commands understand all four roles. Rerank also has a lightweight download path for `gte-multilingual-reranker-base` and persists local rerank model settings correctly.

## Regression Suite

`docs/qa/retrieval_regression_suite.json` is now a 100-case suite built from `Memory_Test/` rather than project-internal development documents. The suite is mostly Chinese and covers all 20 test topics with real target files and original target clues from the corpus.

The suite is designed to test:

- Chinese fact-card retrieval
- semantic/paraphrase recall
- anti-common-answer grounding
- code/name/alias lookup
- multi-document synthesis
- similar-code disambiguation
- PDF/DOCX/TXT/Markdown parsing
- document-type targeting
- typo and colloquial query robustness
- refusal for missing facts, external knowledge, prompt injection, and secret requests
- long multi-condition questions

## Fixes

- Fixed remote/local model policy handling so local-only enterprise mode blocks unauthorized remote runtime usage.
- Fixed rerank configuration persistence in desktop settings.
- Fixed core_docs/live regression scope so live indexing does not feed the entire repository into the embedding service.
- Fixed GBK double-encoding regressions in regression data and documentation workflow.
- Fixed rerank health probing so unavailable rerank is recorded but does not abort the full run.
- Fixed reference excerpt text loading for `.md`, `.markdown`, and `.txt` so normal text references no longer emit misleading parser warnings.
- Preserved the retrieval fallback path when rerank is disabled or unavailable.

## Known Boundaries

- The latest measured 100-case result is strong for the controlled Memory_Test suite, but it is not a claim of 50k-document or production-scale accuracy.
- Refusal behavior is improved but still has residual failures (`91.00%` refuse correctness on the latest live run).
- Some release checklist items remain environment-dependent, especially cross-platform package installation verification.
- Graph remains an explanation/control layer and does not replace document evidence in the primary answer gate.

## Upgrade Notes

- Version is `1.5.0` across the Cargo workspace, UI package, and Tauri desktop config.
- Existing local SQLite data remains local.
- If rerank is not configured, retrieval continues with non-rerank ordering and reports rerank status accordingly.
- For best live retrieval quality, run the four local llama.cpp roles on distinct ports and ensure the rerank endpoint exposes `/v1/rerank`.
