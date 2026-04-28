# Memori-Vault 0.3.0 Release Notes

## English

### Summary

`0.3.0` is a structure-focused release. The main goal is to improve maintainability and team velocity without changing public APIs or core runtime behavior.

### Highlights

- Large-file structural split across first-tier modules:
  - `ui/src/App.tsx` refactored into app-domain modules and panel components.
  - `memori-desktop/src/lib.rs` split into state/runtime/commands modules.
  - `memori-server/src/main.rs` split into state/dto/auth/audit/routes modules.
- Internal code organization improved for handoff and parallel development.
- Documentation updated with structure map and current project status alignment.
- Architecture documentation now points to **Local-first Verifiable Memory OS Lite** as the forward product architecture: SQLite local-first storage, verifiable evidence, Evidence Firewall, MCP memory tools, Trust Panel, and layered memory.

### Forward Architecture Note

`0.3.0` should not be described as a completed Memory OS release. The current branch has partial implementation of Memory Domain v1, MCP memory tools, source grouping, evidence compression, and Trust Panel, while the 50-case accuracy gate, temporal graph, Markdown source-of-truth, heat score, and lifecycle classifier remain active work.

### Compatibility

- No intended breaking changes for:
  - Tauri command names and payload shape
  - Server route paths/methods
  - Existing ask/retrieval protocol contracts

---

## 中文

### 概要

`0.3.0` 是一次以结构治理为主的版本。重点是提升可维护性和协作效率，不改变对外接口与核心运行时行为。

### 主要更新

- 第一梯队大文件完成结构拆分：
  - `ui/src/App.tsx` 按应用域与面板组件拆分。
  - `memori-desktop/src/lib.rs` 拆分为状态/运行时/命令模块。
  - `memori-server/src/main.rs` 拆分为状态/DTO/鉴权/审计/路由模块。
- 内部模块边界更清晰，便于后续并行开发与 AI 交接。
- 文档结构地图与阶段状态同步更新。

### 兼容性

- 预期无破坏性变化：
  - Tauri command 名称与参数形状不变
  - Server 路由路径与方法不变
  - 现有 ask/retrieval 协议约定不变
