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
2. 在 **基础** 页选择监听目录与 Top-K。
3. 在 **模型** 页选择 provider，并填写 endpoint / key / 三角色模型。
4. 点击 **测试连接**，确认可用后 **保存配置**。

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
2. 查看 SYNTHESIS 与来源卡片。
3. 用“范围选择”缩小到指定文件/目录，提高准确率与效率。

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
- 统计一直 0：检查目录是否有效、索引是否暂停、是否需要手动重建。
- 表格显示异常：通常是分块边界把 Markdown 表格切断，建议缩小检索范围。
- 窗口位置异常：新版已做脏状态过滤，必要时清理本地窗口持久化字段。

## 7. 发版前检查

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `npm --prefix ui run build`
- 版本一致性：workspace / tauri / ui package
- 发布说明：`docs/RELEASE_NOTES_v0.2.0.md`
