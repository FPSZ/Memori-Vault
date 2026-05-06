# Memori-Vault 1.0.2 Release Notes

## Summary

`1.0.2` is a stabilization release for the current Memori-Vault desktop stack. It keeps the existing local-first, verifiable retrieval architecture and packages a smaller set of fixes aimed at release usability, runtime clarity, and delivery consistency.

This release remains within the **Memory OS Lite** product boundary: local-first storage, verifiable retrieval, explicit evidence separation, and auditable agent-facing knowledge access.

## Highlights

- Unified the published version to `1.0.2` across the Cargo workspace, UI package, and Tauri desktop config.
- Preserved the local-first runtime path built around SQLite, layered retrieval, and chunk-level evidence.
- Kept the current split between document evidence, memory context, and graph explanation surfaces.
- Prepared the release line for continued desktop delivery without changing the product boundary or retrieval contract.

## Current Product Boundary

Memori-Vault remains positioned as a local-first knowledge retrieval and long-term memory system for private document libraries, project context, and agent workflows.

The current stable product line continues to emphasize:

- local SQLite-backed storage
- verifiable chunk-level citations
- mixed Chinese / English / code-token retrieval support
- MCP-based agent integration
- explicit separation between document evidence and memory context

## Notes

- This release is a versioned packaging and stabilization increment, not a major architecture reset.
- Retrieval quality and local model validation should still be measured with the existing QA and regression workflow in `docs/qa/`.
- Graph remains an explanation layer and does not replace the primary document evidence gate.
