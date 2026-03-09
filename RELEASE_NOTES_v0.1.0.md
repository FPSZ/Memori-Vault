# Memori-Vault 0.1.0 Release Notes

## English

### Highlights

- First public desktop build based on Tauri v2 + React.
- End-to-end local-first memory pipeline:
  - file watch (`.md` / `.txt`)
  - semantic chunking
  - embedding retrieval
  - graph extraction
  - SQLite persistence
- Settings center with in-app workflow:
  - UI language and AI answer language (separate)
  - watch folder switching
  - retrieval Top-K control
  - personalization options
- Source cards support markdown preview, expand/collapse, and open file location.
- Browser-compatible server mode (`memori-server`).
- Cross-platform release workflow with draft release automation.

### Runtime Requirements

- Ollama local runtime for local provider mode.
- Recommended local models:
  - `nomic-embed-text:latest`
  - `qwen2.5:7b`

---

## Post-0.1.0 Updates (Current Mainline)

### Performance & Indexing

- Refactored indexing for first-answer speed:
  - fast searchable chunk persistence first
  - deferred async graph queue processing
- Added indexing modes:
  - `continuous | manual | scheduled`
- Added resource budgets:
  - `low | balanced | fast`
- Added controls and status APIs:
  - get status
  - set mode/budget/window
  - trigger reindex
  - pause/resume

### UX Updates

- Added indexing controls in settings Advanced tab.
- Added query elapsed timer during loading and final elapsed time on synthesis header.
- Continued dark/light token consistency improvements.

### Behavior Notes

- Graph build is intentionally non-blocking for retrieval responses.
- Unchanged files are skipped via metadata/hash checks to reduce recompute.

---

## 中文

### 版本亮点

- 首个可用的公开桌面版本（Tauri v2 + React）。
- 本地优先核心链路可用：
  - 文件监听（`.md` / `.txt`）
  - 语义分块
  - 向量检索
  - 图谱抽取
  - SQLite 持久化
- 设置中心支持：
  - UI 语言与 AI 回答语言分离
  - 读取目录切换
  - Top-K 调节
  - 个性化选项
- 来源卡片支持 Markdown 预览、展开/折叠、打开文件位置。
- 支持浏览器模式（`memori-server`）。
- 支持三端自动构建与草稿发布。

### 运行要求

- 本地模式需运行 Ollama。
- 推荐模型：
  - `nomic-embed-text:latest`
  - `qwen2.5:7b`

---

## 0.1.0 后主线更新（当前）

### 性能与索引

- 索引重构为首问优先：
  - 先写可检索分块
  - 图谱改为后台异步队列补齐
- 新增索引模式：
  - `continuous | manual | scheduled`
- 新增资源档位：
  - `low | balanced | fast`
- 新增索引控制与状态接口：
  - 获取状态
  - 设置模式/档位/窗口
  - 手动重建
  - 暂停/恢复

### 体验更新

- 设置页高级分组接入索引控制面板。
- 检索时显示实时耗时，完成后在 SYNTHESIS 标题显示总耗时。
- 持续优化深浅主题 token 一致性。

### 行为说明

- 图谱构建不再阻塞检索回答。
- 对未变化文件执行跳过策略，减少重复计算。
