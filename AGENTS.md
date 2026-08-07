# AGENTS.md — AI 协作规则（Memori-Vault）

> 本文件供所有 AI 编程助手（Cursor / Codex / Copilot 等支持 `AGENTS.md` 的工具）自动读取。
> 完整交接说明见 [`交接说明.md`](./交接说明.md)。

## 项目一句话

Memori-Vault：Rust(Tauri) + React 的**本地优先、可验证**长期记忆引擎（本地知识库问答，答案可追溯到来源文件/片段）。当前版本 v1.5.2，Apache 2.0。

## ⛔ 最高优先级规则（不可违反）

1. **所有开发只在 `collab` 分支进行。**
2. 执行任何 git 操作或写文件**之前**，先运行 `git branch --show-current` 确认当前分支是 `collab`；不是就先 `git checkout collab`。
3. **禁止** checkout / commit / merge / rebase / push 到 `main` 或 `dev`。
4. **禁止** `git push --force`；**禁止**对 `main` / `dev` 做任何合并或变基。
5. 若任务看起来需要动 `main` / `dev`（如同步上游），**停下来交给人类**，不要自己动手。

## 工作约定

- commit message 用清晰的中文描述改动。
- `.ps1` 脚本必须存成 **UTF-8 with BOM**（Windows PowerShell 5.1 否则中文乱码）。
- 不确定就先问人类，不要擅自删数据或改动 `main`/`dev`。

## 怎么跑

- Windows 一键：`.\dev.ps1`（`-ServerOnly` / `-DesktopOnly`）。
- 手动：`pnpm --dir ui install && pnpm --dir ui run dev -- --host 127.0.0.1 --port 1420 --strictPort` + `cargo tauri dev -p memori-desktop`。
- 仅服务端：`cargo run -p memori-server`（默认 `http://127.0.0.1:3757`，MCP 在 `/mcp`）。
