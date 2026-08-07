# CLAUDE.md — Claude Code 项目规则（Memori-Vault）

> Claude Code 会自动读取本文件。完整规则见 [`AGENTS.md`](./AGENTS.md) 与 [`交接说明.md`](./交接说明.md)。

## ⛔ 最高优先级规则（不可违反）

**本项目所有开发只在 `collab` 分支进行。**

- 执行任何 git 操作或写文件**之前**，先 `git branch --show-current` 确认在 `collab`；不是就 `git checkout collab`。
- **禁止** checkout / commit / merge / rebase / push 到 `main` 或 `dev`。
- **禁止** `git push --force`，**禁止**对 `main` / `dev` 做合并或变基。
- 若任务需要动 `main` / `dev`，**停下来交给人类**。

## 项目速览

Memori-Vault：Rust(Tauri) + React 的本地优先可验证长期记忆引擎（本地知识库问答，答案可追溯来源）。v1.5.2，Apache 2.0。

- 启动：`.\dev.ps1`（Windows 一键）或 `cargo tauri dev -p memori-desktop` + 前端 `pnpm --dir ui run dev`。
- 仅服务端：`cargo run -p memori-server`（`http://127.0.0.1:3757`，MCP 在 `/mcp`）。
- `.ps1` 文件必须存成 **UTF-8 with BOM**，否则 PowerShell 5.1 中文乱码。
- 架构文档：`docs/architecture/MEMORY_OS_LITE.md`、`docs/architecture/STRUCTURE.md`。
