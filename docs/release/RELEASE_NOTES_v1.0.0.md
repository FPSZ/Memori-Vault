# Memori-Vault 1.0.0 Release Notes

## Summary

`1.0.0` is the first stable delivery milestone for Memori-Vault as a **Memory OS Lite**: a local-first, verifiable knowledge retrieval and long-term memory system. The release focuses on making private team and enterprise document libraries searchable, attributable, and controllable without requiring cloud upload.

## Highlights

- Stabilized the local llama.cpp model runtime path for separate chat, graph, and embedding services.
- Improved model start/stop diagnostics, including per-role logs and clearer port/configuration errors.
- Added graph data APIs and the first interactive knowledge graph surface.
- Improved PDF/DOCX parsing and preview behavior for document-oriented reading.
- Added storage-level repair for empty FTS tables so existing chunks can restore lexical retrieval without rerunning embeddings.
- Improved indexing state handling so partially failed retryable files do not invalidate already indexed chunks.
- Expanded settings UX around model configuration, indexing scope, logging, and save actions.
- Added regression coverage for CJK mixed identifier retrieval such as internal project-code questions.

## Retrieval And Indexing

The main retrieval path remains document routing, chunk retrieval, evidence merge, gating, citation, then answer generation. This release fixes a critical main-path issue where existing chunks could remain present while `chunks_fts` and `documents_fts` were empty after migration or rebuild interruption. Store initialization now repairs those FTS tables from existing chunk/document rows, preserving the normal lexical retrieval path.

## Upgrade Notes

- Version is now `1.0.0` across the Cargo workspace, UI package, and Tauri desktop config.
- Existing local SQLite data remains local.
- If embeddings already exist but FTS rows are missing, the database repair runs on startup and does not require rerunning the embedding model.
- If files remain in a retryable failed state because the embedding model is offline, already indexed chunks remain searchable.

## Known Boundaries

- Large-scale retrieval quality should continue to be measured with `docs/qa/retrieval_regression_suite.json`.
- Graph ranking remains an explanation/control surface and should not replace document evidence in the primary retrieval gate.
- Markdown export remains a planned capability rather than a completed release feature.
