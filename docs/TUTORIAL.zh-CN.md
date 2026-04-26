# Memori-Vault 教程（中文辅助）

英文主教程（推荐优先阅读）：[TUTORIAL.md](./TUTORIAL.md)

> 说明：本页是中文辅助版，结构与英文主教程对齐，但内容更精简。

## 1. 准备条件

- 桌面版（推荐）或 `memori-server` 模式。
- 一个知识目录（当前支持 `.md`、`.txt`）。
- 模型运行环境：
  - 本地优先：Ollama。
  - 远程：OpenAI-compatible endpoint + API key。

## 2. 首次配置（桌面版）

1. 打开设置（右上角齿轮）。
2. 如果当前还没完成模型配置：
   - 应用不会自动启动本地 Ollama 或远端 runtime。
   - 搜索框会保持禁用。
   - 搜索框位置会显示红色内联提示：`未配置模型，请在 设置 > 模型 中配置`
3. 在 **基础** 页选择监听目录与 Top-K。
4. 在 **模型** 页选择 **本地 Ollama** 或 **远程 API**，并填写 endpoint / key / 三角色模型。
5. 点击 **测试连接**，确认可用后 **保存配置**。

结果：
- 当前 active provider 配置完整且可用后，搜索框会立即恢复可编辑。

## 3. 推荐本地模型

- `chat_model`: `qwen2.5:7b`
- `graph_model`: `qwen2.5:7b`
- `embed_model`: `nomic-embed-text:latest`
- endpoint: `http://localhost:11434`

检查：
```bash
ollama list
```

## 4. 使用流程

1. 输入问题并检索。
2. 查看“回答 / 引用 / 证据 / 检索指标”四块结果。
3. 用“范围选择”缩小到指定文件/目录，提高准确率与效率。

说明：
- 引用默认折叠，需要时再展开查看原文。
- 证据卡片会先按文档聚合并去重，不再直接把重复 chunk 全部摊开。
- 检索指标会展示阶段耗时，以及总耗时 / 已打点小计 / 未打点部分。

## 5. 索引策略（高级）

- `continuous`：持续后台索引（默认）
- `manual`：手动触发
- `scheduled`：按时间窗执行

资源档位：
- `low`（推荐日常）
- `balanced`
- `fast`

## 6. 常见问题

- 连接失败：检查 endpoint 路径和 key，切换 provider 后重新测试。
- 远端 provider 也必须把 `chat / graph / embed` 三个角色都配完整。
- 统计一直 0：检查目录是否有效、索引是否暂停、是否需要手动重建。
- 搜索框不可用：通常表示当前 active provider 还没完成配置；去 **设置 > 模型** 保存完整配置即可。
- 表格显示异常：通常是分块边界把 Markdown 表格切断，建议缩小检索范围。
- 窗口位置异常：新版已做脏状态过滤，必要时清理本地窗口持久化字段。

## 7. 发版前检查

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `pnpm --dir ui run build`
- 版本一致性：workspace / tauri / ui package
- 发布说明：`docs/RELEASE_NOTES_v0.4.0.md`

## 8. 可选烟测脚本

- 启动桌面 / 服务端烟测：
```powershell
.\scripts\smoke-start.ps1
```

- 关闭烟测服务：
```powershell
.\scripts\smoke-stop.ps1
```

- 跑外部语料可用性 smoke：
```powershell
.\scripts\test-usability-smoke.ps1 -CorpusRoot <你的语料目录>
```

补充：
- 这些脚本只是本地验证入口，不是产品协议的一部分。
- `smoke-start.ps1` 现在支持跳过本地模型检查，便于单独验证 UI / server 流程。
# Memory OS Lite 使用提示

Memori-Vault 当前架构定位是 **Local-first Verifiable Memory OS Lite**，详细设计见 [MEMORY_OS_LITE.md](./MEMORY_OS_LITE.md)。

使用时请重点关注：

- 答案区不只看正文，还要看 citation、evidence、Trust Panel 和 retrieval metrics。
- Trust Panel 会展示 `answer_source_mix`、`failure_class`、`source_groups`、`memory_context` 和 `context_budget_report`。
- 对话/项目记忆可以帮助回答，但只能作为 `memory_context`，不能冒充文档 citation。
- 设置页的 Memory 选项用于控制 conversation memory、auto memory write、source requirement、Markdown export 和 context budget。
- MCP endpoint 默认为 `http://127.0.0.1:3757/mcp`，agent 可通过 `ask/search/get_source/open_source` 和 `memory_search/memory_add/memory_update` 等工具调用本地知识库。
