# Retrieval Baseline Snapshot

Updated: 2026-03-11 UTC

## Architecture Notes Since This Baseline

The retrieval baseline should now be read together with [MEMORY_OS_LITE.md](./MEMORY_OS_LITE.md). The current architecture adds Memory OS Lite fields around the existing retrieval path without changing the core rule that document citations must come from document chunks.

New structured result fields to track in future reports:

- `answer_source_mix`: `document_only`, `document_plus_memory`, `memory_only`, or `insufficient`.
- `memory_context`: conversation/project/preference memory used as context, not as citation.
- `source_groups`: duplicate or sibling source aggregation, especially for `.txt/.md` paired documents.
- `failure_class`: `recall_miss`, `rank_miss`, `gating_false_negative`, `generation_refusal`, `citation_miss`, or `none`.
- `context_budget_report`: token budget split across document evidence, memory, graph context, and answer prompt.

Future 50-case reports must count memory context separately from citation validity. A query answered from memory-only can be valid for project-memory intent, but it must not be counted as a document citation hit.

This document records the current retrieval and enterprise-local-first baseline for the code that exists in this branch today.

Canonical regression suite source:

- `docs/retrieval_regression_suite.json`
- We do not keep a second Markdown mirror of the suite cases in-repo anymore.

Important boundary:

- The numbers below come from the checked-in regression corpora only.
- They are not a 50,000-document validation result.
- Current offline baselines cover:
  - `core_docs`: 6 indexed documents
  - `repo_mixed`: 11 indexed documents
- Current mixed-corpus retrieval quality is still below a delivery bar for confident external claims.

## 1. Current Retrieval Path

### Desktop structured ask path
1. `ui/src/App.tsx` calls `ask_vault_structured`.
2. `memori-desktop/src/lib.rs` normalizes `scope_paths` and validates runtime model policy before use.
3. `memori-core/src/lib.rs` runs `ask_structured(...)`.
4. `ask_structured(...)` first calls `retrieve_structured(...)`.
5. `retrieve_structured(...)` executes:
   - query analysis
   - document routing via `documents_fts` plus deterministic filename/path/symbol signals
   - chunk lexical retrieval via `search_chunks_fts(...)`
   - chunk dense retrieval via `search_similar_scoped(...)`
   - chunk-level merge and dedupe
   - strong-evidence gating
6. If evidence is sufficient, the answer step runs.
7. The result returns as structured JSON:
   - `status`
   - `answer`
   - `citations`
   - `evidence`
   - `metrics`
8. UI renders citations and evidence directly and no longer parses a plain-text source tail block.

### Server structured ask path
1. `POST /api/ask` accepts the ask request.
2. Server validates enterprise runtime policy before runtime use.
3. Server calls the same core `ask_structured(...)` path.
4. `/api/ask_legacy` remains compatibility-only and is derived from the structured response.

## 2. Formal Regression Entry Points

### Offline deterministic
```bash
cargo run -p memori-core --example retrieval_regression -- --suite docs/retrieval_regression_suite.json --watch-root . --mode offline_deterministic --profile core_docs
cargo run -p memori-core --example retrieval_regression -- --suite docs/retrieval_regression_suite.json --watch-root . --mode offline_deterministic --profile repo_mixed
```

### Live embedding
```bash
cargo run -p memori-core --example retrieval_regression -- --suite docs/retrieval_regression_suite.json --watch-root . --mode live_embedding --profile full_live
```

Report output:

- JSON source of truth: `target/retrieval-regression/<mode>-<profile>-<timestamp>/report.json`
- Human review copy: `target/retrieval-regression/<mode>-<profile>-<timestamp>/report.md`

## 3. Runtime Baseline Snapshot

### Offline deterministic `core_docs`

| Field | Value |
| --- | --- |
| `watch_root` | `D:\Create\tools\Memor-Vault\.` |
| `scope` | suite-driven; empty scope resolves to current `watch_root` |
| `db_path` | `target/retrieval-regression/offline_deterministic-core_docs.db` |
| `embedding_model_key` | `nomic-embed-text:latest` |
| `embedding_dim` | `256` |
| `indexed_document_count` | `6` |
| `indexed_chunk_count` | `267` |
| `rebuild_state` | `ready` |
| `service_health` | `ready` |
| `index_prep_ms` | `304` |

Report:

- `target/retrieval-regression/offline_deterministic-core_docs-1773229611/report.json`

### Offline deterministic `repo_mixed`

| Field | Value |
| --- | --- |
| `watch_root` | `D:\Create\tools\Memor-Vault\.` |
| `scope` | suite-driven; empty scope resolves to current `watch_root` |
| `db_path` | `target/retrieval-regression/offline_deterministic-repo_mixed.db` |
| `embedding_model_key` | `nomic-embed-text:latest` |
| `embedding_dim` | `256` |
| `indexed_document_count` | `11` |
| `indexed_chunk_count` | `759` |
| `rebuild_state` | `ready` |
| `service_health` | `ready` |
| `index_prep_ms` | `1301` |

Report:

- `target/retrieval-regression/offline_deterministic-repo_mixed-1773229598/report.json`

### Live embedding `full_live`

| Field | Value |
| --- | --- |
| `watch_root` | `D:\Create\tools\Memor-Vault\.` |
| `scope` | suite-driven; `full_live` |
| `db_path` | `target/retrieval-regression/live_embedding-full_live.db` |
| `embedding_model_key` | `nomic-embed-text:latest` |
| `embedding_dim` | `0` |
| `indexed_document_count` | `0` |
| `indexed_chunk_count` | `0` |
| `rebuild_state` | `ready` |
| `service_health` | `unavailable` |
| `index_prep_ms` | `N/A` |
| `preparation_error` | `embedding provider probe failed: Embedding 请求失败: error sending request for url (http://localhost:11434/api/embeddings)` |

Status:

- Blocked by local model availability on `http://localhost:11434`
- This is a valid structured live-baseline outcome and must not be replaced with synthetic metrics

Report:

- `target/retrieval-regression/live_embedding-full_live-1773222132/report.json`

## 4. Measured Retrieval Metrics

### Offline deterministic results

| Profile | Cases | Answer Cases | Refuse Cases | Indexed Documents | Top-1 Document Hit | Top-3 Document Recall | Top-5 Chunk Recall | Citation Validity | Reject Correctness |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `core_docs` | 39 | 33 | 6 | 6 | 0.6970 | 0.6970 | 0.7576 | 1.0000 | 1.0000 |
| `repo_mixed` | 50 | 44 | 6 | 11 | 0.4773 | 0.4773 | 0.5455 | 1.0000 | 0.9400 |

### Live embedding results

| Profile | Cases Executed | Service Health | Result |
| --- | ---: | --- | --- |
| `full_live` | 0 | `unavailable` | blocked by local Ollama / embedding probe failure |

## 5. Current Interpretation

### What is solid today

1. Structured retrieval, citations, evidence output, and policy-gated runtime are implemented.
2. Citation validity remains `100%` in both offline profiles.
3. `core_docs` is usable as a narrow documentation-only baseline.

### What is not yet good enough

1. `repo_mixed` is not at an acceptable precision bar.
   - Latest measured result: `Top-1=0.4773`
   - That means on the current 11-document mixed regression corpus, the correct document is ranked first less than half the time.
2. `repo_mixed` regressed versus the previous documented offline snapshot.
   - previous documented snapshot: `Top-1=0.5682`, `Top-3=0.5909`, `Top-5=0.6364`, `Reject=0.9800`
   - latest snapshot: `Top-1=0.4773`, `Top-3=0.4773`, `Top-5=0.5455`, `Reject=0.9400`
3. Live local-model validation is still blocked by local Ollama or local embedding availability.
4. No current document should imply that retrieval precision has been validated at 50,000-document scale.

### Practical delivery posture

- `core_docs`: internal validation baseline, useful for continued regression work.
- `repo_mixed`: still beta/internal-only quality, not ready for strong accuracy claims.
- `full_live`: runtime path exists, but the local-model environment has not been validated end to end on this machine.

## 6. Remaining Failure Pattern Summary

Current dominant failure classes are:

1. Descriptive English documentation queries still route to the wrong document.
   - examples: `R02`, `R05`, `R13`, `R21`, `R28`, `R35`, `R36`
2. Mixed code and implementation lookup still under-ranks the correct file.
   - examples: `R40`, `R42`, `R43`, `R44`, `R45`, `R46`, `R50`, `R51`
3. Some answerable queries still get rejected in `repo_mixed`.
   - examples: `R19`, `R35`, `R42`

Most important point:

- The current trust story is better than the current ranking story.
- We can trust citations when the system answers.
- We cannot yet claim that mixed-corpus document routing is consistently strong.

## 7. Enterprise Local-First Runtime Baseline

Current implementation baseline:

- Default enterprise egress mode is `local_only`.
- Shared policy validation lives in `memori-core` and is enforced by both desktop and server.
- Remote configuration may remain editable in UI, but activation, probe, model listing, pull, engine startup, and ask/index runtime use are policy-gated.
- Environment variables are resolved into the runtime candidate and then validated by enterprise policy; they can tighten or redirect locally, but they cannot bypass `local_only` or `allowlist`.

Acceptance status in this baseline:

- Desktop and server policy gates are implemented.
- Enterprise runtime policy posture is ahead of retrieval quality posture.
- Retrieval quality for mixed corpora remains a separate validation item and is not implied by policy enforcement.

## 8. Immediate Conclusions

- Phase 0 runtime baseline and measured offline retrieval metrics are captured.
- Current offline evidence does not support a claim that the system can reliably pinpoint one correct document out of a large mixed corpus.
- The next retrieval work should focus on mixed-corpus document routing precision before any broader quality claim.
- Live embedding baseline remains blocked until the local Ollama environment is available.
