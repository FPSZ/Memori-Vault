# Memori-Vault 0.1.0 Release Notes

## English

### Highlights

- First public desktop build based on **Tauri v2 + React**.
- End-to-end **local-first memory pipeline** is available:
  - file watch (`.md` / `.txt`)
  - semantic chunking
  - embedding retrieval
  - graph extraction
  - SQLite persistence
- New **settings center** with polished in-app workflow:
  - UI language and AI answer language (separate controls)
  - watch folder switching (runtime update)
  - retrieval Top-K control
  - personalization options
- Source cards now support:
  - markdown preview for `.md`
  - expand/collapse content
  - open file location from the card
- Added **browser-compatible server mode** (`memori-server`) for non-desktop/headless scenarios.
- Cross-platform release workflow for **Windows / Linux / macOS** with draft release automation.

### Runtime Requirements

- Ollama must be running locally.
- Recommended models:
  - `nomic-embed-text:latest` (embedding)
  - `qwen2.5:7b` (chat / graph extraction)

### Notes

- Linux bundle size is larger than Windows installer size due to platform packaging/runtime dependencies.
- If you run UI without Tauri host, use server mode (`memori-server`) for API calls.

---

## 中文

### 版本亮点

- 首个可用的公开桌面版本，基于 **Tauri v2 + React**。
- 本地优先的核心记忆链路已完整可用：
  - 文件监听（`.md` / `.txt`）
  - 语义分块
  - 向量检索
  - 图谱抽取
  - SQLite 持久化
- 新增高质感**设置中心**，支持应用内完整配置流程：
  - UI 语言与 AI 回答语言（分离设置）
  - 读取目录切换（运行时生效）
  - 检索 Top-K 数量调节
  - 个性化选项
- 来源卡片能力增强：
  - `.md` 文件 Markdown 预览
  - 内容展开 / 折叠
  - 卡片内直接打开文件位置
- 新增可浏览器运行的 **Server 模式**（`memori-server`），适配无桌面壳或 Headless 场景。
- 提供 **Windows / Linux / macOS** 自动构建与 Draft Release 发布流程。

### 运行要求

- 需本地运行 Ollama。
- 推荐模型：
  - `nomic-embed-text:latest`（向量模型）
  - `qwen2.5:7b`（问答 / 图谱抽取模型）

### 说明

- Linux 安装包体积通常显著大于 Windows，属于平台打包与运行时依赖差异。
- 如果只运行前端而未启动 Tauri，请改用 `memori-server` 提供后端接口。
