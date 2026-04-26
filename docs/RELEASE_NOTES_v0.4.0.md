# Memori-Vault 0.4.0 Release Notes

## Summary

`0.4.0` moves Memori-Vault from a local RAG desktop application toward **Local-first Verifiable Memory OS Lite**. The release keeps the SQLite/local-first foundation and adds the first practical layer of agent memory, MCP control, trust reporting, and product documentation around verifiable evidence.

This release should be described as an architecture and capability expansion, not as a completed high-scale accuracy milestone. The 50-case acceptance target remains active: `answered >= 45/50`, `correct >= 40/50`, and `citation/source_group_hit >= 45/50`.

## Highlights

- Repositioned the product as **Local-first Verifiable Memory OS Lite**.
- Added a canonical architecture document: `docs/MEMORY_OS_LITE.md`.
- Expanded official MCP toward an agent memory/control interface.
- Added Memory Domain v1 concepts for events, memories, lifecycle logging, update, and supersede flows.
- Added memory-oriented MCP tools such as `memory_search`, `memory_add`, `memory_update`, `memory_list_recent`, and `memory_get_source`.
- Added ask-time memory context fields so document evidence and memory context can be reported separately.
- Added Evidence Firewall semantics: document citations must come from document chunks; conversation/project memory is returned as memory context.
- Added Trust Panel documentation and release gate coverage for `answer_source_mix`, `failure_class`, `source_groups`, `memory_context`, and `context_budget_report`.
- Improved README positioning around evidence, local SQLite, CJK/mixed-token retrieval, MCP, graph explanation, and private deployment.
- Updated deployment and enterprise docs to describe local-first governance, MCP endpoint usage, audit expectations, and memory write source requirements.

## MCP And Agent Memory

The official MCP surface is intended to be the stable agent entry point. It covers:

- Query and evidence: `ask`, `search`, `get_source`, `open_source`.
- Memory: `memory_search`, `memory_add`, `memory_update`, `memory_list_recent`, `memory_get_source`.
- Runtime control: indexing, model settings, app settings, graph exploration, and diagnostics.

Full-control MCP remains a local-first capability. Long-term memory writes should be source-bound, auditable, and reversible through lifecycle records.

## Retrieval And Trust

The main retrieval chain remains:

```text
document routing -> chunk retrieval -> RRF/gating -> evidence/citation
```

Graph and memory are context/explanation layers by default. They should not replace document evidence in P0 quality gates.

Structured ask responses should expose:

- `answer_source_mix`
- `memory_context`
- `source_groups`
- `failure_class`
- `context_budget_report`

## Known Boundaries

- Temporal graph visualization is still in progress.
- Markdown source-of-truth/export is still in progress.
- Memory heat score, conflict resolver, and lifecycle classifier are still in progress.
- The 50-case accuracy gate is still the release-quality target, not a completed claim.
- 50k-document high-precision validation is not claimed for this release.
- Full `cargo test -p memori-core` may still contain older CJK retrieval assertions that need stabilization; release CI should report the authoritative status for the pushed commit.

## Upgrade Notes

- Version is now `0.4.0` across Cargo workspace, UI package, and Tauri config.
- Existing local data should remain local. If a schema/index format change requires rebuild, search may be temporarily unavailable until reindex completes.
- MCP clients should prefer the official endpoint and tool names instead of direct internal REST assumptions.

