# Remote Model Access Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make remote OpenAI-compatible model access reliable and add reusable remote presets.

**Architecture:** Keep `memori-core` on its existing OpenAI-compatible transport. Align server DTO/settings/runtime with desktop/UI three-role endpoints, then add UI presets that fill or save remote profiles without changing local llama.cpp behavior.

**Tech Stack:** Rust server/desktop DTOs, React/TypeScript settings UI, existing Tauri command flow.

---

### Task 1: Server Remote Endpoint Alignment

**Files:**
- Modify: `memori-server/src/dto.rs`
- Modify: `memori-server/src/model_runtime.rs`
- Modify: `memori-server/src/routes/models.rs`
- Modify: `memori-server/src/mcp/tools_impl.rs`

- [ ] Add `remote_chat_endpoint`, `remote_graph_endpoint`, and `remote_embed_endpoint` to persisted server settings.
- [ ] Change server remote model profile to expose `chat_endpoint`, `graph_endpoint`, and `embed_endpoint`, while accepting legacy `endpoint`.
- [ ] Resolve active runtime with role-specific remote endpoints and set role-specific env vars.
- [ ] Save all three remote endpoints through HTTP and MCP settings paths.

### Task 2: UI Remote Provider Presets

**Files:**
- Modify: `ui/src/components/settings/types.ts`
- Modify: `ui/src/components/settings/tabs/modelUtils.ts`
- Modify: `ui/src/components/settings/tabs/ModelsTab.tsx`

- [ ] Add built-in OpenAI-compatible provider presets for common services.
- [ ] Add local-storage backed user presets using the existing model tab card/button/input styling.
- [ ] Allow saving the current remote profile as a named preset and applying a saved preset.
- [ ] Keep duplicate endpoint validation local-only.

### Task 3: Verification

**Files:**
- Modify tests in existing Rust files if needed.

- [ ] Add or update tests proving remote runtime uses three remote endpoints, not local defaults.
- [ ] Run Rust tests for server/desktop model settings.
- [ ] Run the UI build.
- [ ] Commit and push to `origin/dev`.
