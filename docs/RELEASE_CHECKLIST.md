# Memori-Vault Release Checklist

## Memory OS Lite Release Gate

- [ ] README uses product positioning, not change-log wording: problem, why not other RAG tools, core advantages, quick start, architecture, current boundaries.
- [ ] [MEMORY_OS_LITE.md](./MEMORY_OS_LITE.md) reflects current implementation and pending work.
- [ ] Trust Panel displays `answer_source_mix`, `failure_class`, `source_groups`, `memory_context`, and `context_budget_report`.
- [ ] Evidence Firewall is verified: document citations only come from document chunks.
- [ ] MCP `tools/list` includes query/source tools and memory tools.
- [ ] `memory_add` / `memory_update` writes are source-bound or policy-rejected and produce lifecycle/audit entries.
- [ ] 50-case acceptance report is attached or explicitly marked pending.
- [ ] Release notes do not claim temporal graph, Markdown source-of-truth, heat score, or 50k-document validation as complete unless verified.

这份清单用于桌面版发布前的最终确认，目标是把“能构建”提升为“可对外发布”。

## 1. 版本与文档

- [ ] `Cargo.toml` 中 `workspace.package.version` 已更新
- [ ] `ui/package.json` 中 `version` 已更新
- [ ] `memori-desktop/tauri.conf.json` 中 `version` 已更新
- [ ] 以上三个版本号保持一致
- [ ] 已编写对应版本的 release notes：`docs/RELEASE_NOTES_vX.Y.Z.md`
- [ ] `README.md` 与 `README.zh-CN.md` 的运行模式和企业版口径已同步
- [ ] `docs/enterprise.md` 与 `docs/enterprise.zh-CN.md` 的 preview/GA 口径已同步

## 2. 代码质量门

- [ ] 运行 `cargo fmt --all -- --check`
- [ ] 运行 `cargo clippy --workspace -- -D warnings`
- [ ] 运行 `cargo test --workspace`
- [ ] 运行 `pnpm --dir ui install --frozen-lockfile`
- [ ] 运行 `pnpm --dir ui run build`
- [ ] 主 CI（`rust-ci.yml`）已通过

## 3. 发布关键行为验证

- [ ] 新增/修改 `.md` 或 `.txt` 文件后可正常进入索引
- [ ] 删除单个文档后，旧内容不会继续出现在检索结果中
- [ ] 将 `.md/.txt` 重命名为不支持后缀后，旧索引会被清理
- [ ] 删除目录后，目录内旧索引会被清理
- [ ] 当 parser / index 语义版本不兼容时，系统会自动进入 `required/rebuilding`
- [ ] 在 `required/rebuilding` 期间，search / ask 会被显式拒绝，而不是继续读取旧索引
- [ ] 全量重建完成后，`rebuild_state` 会恢复为 `ready`
- [ ] 索引状态不会卡在 `scanning` / `embedding`
- [ ] 设置中心可正常保存关键配置

## 4. 运行时与产品口径

- [ ] 桌面版作为当前主体验进行发布
- [ ] `memori-server` 口径保持为 server runtime / private deployment preview
- [ ] 企业能力口径保持为 preview，不对外宣称完整 GA 级企业身份安全能力
- [ ] Ollama 依赖与推荐模型说明清晰

## 5. 企业策略运行态验收

### Automated evidence snapshot (2026-03-11 UTC)

- `cargo test -p memori-core --lib` 已通过
- `cargo check -p memori-core -p memori-desktop -p memori-server` 已通过
- `pnpm --dir ui run build` 已通过
- offline regression 已跑通：
  - `offline_deterministic + core_docs`
  - `offline_deterministic + repo_mixed`
  - 最新报告：
    - `target/retrieval-regression/offline_deterministic-core_docs-1773229611/report.json`
    - `target/retrieval-regression/offline_deterministic-repo_mixed-1773229598/report.json`
  - 当前结果：
    - `core_docs`: `Top-1=0.6970`、`Top-3=0.6970`、`Top-5=0.7576`、`citation validity=1.0`、`reject correctness=1.0`
    - `repo_mixed`: `Top-1=0.4773`、`Top-3=0.4773`、`Top-5=0.5455`、`citation validity=1.0`、`reject correctness=0.94`
- live regression 已产出结构化失败报告：
  - `live_embedding + full_live`
  - 当前阻塞：本地 Ollama / embedding endpoint `http://localhost:11434` 不可达

### Current release posture

- 企业本地优先运行时与策略阻断链路已经有实现和文档闭环。
- 检索质量和企业策略是两条不同验收线，不能互相替代。
- 当前 mixed corpus 离线精度仍然偏低，不适合写成“可稳定从整个资料库精确定位目标文档”的对外口径。
- 若当前发版，检索能力更适合作为 internal preview / beta 口径，而不是高精度知识检索已完成的口径。

### Server API checklist

- [ ] `GET /api/admin/policy` 可读取当前 enterprise policy
  Expected result: 返回 `egress_mode`、`allowed_model_endpoints`、`allowed_models`，且默认值符合 `local_only`
- [ ] `PUT /api/admin/policy` 更新后立即触发 engine re-evaluation
  Expected result: policy 更新后新的 ask / model runtime 使用新策略，不继续沿用旧 runtime
- [ ] `local_only` 下保存远端 runtime 被拒绝
  Expected result: `POST /api/model-settings` 返回明确 forbidden / policy message
- [ ] `local_only` 下远端 probe / list 被拒绝
  Expected result: `POST /api/model-settings/probe` 与 `POST /api/model-settings/list-models` 返回明确策略阻断，而不是普通网络错误
- [ ] `allowlist` 下非白名单 endpoint 被拒绝
  Expected result: 返回 `remote_endpoint_not_allowlisted` 或等价策略错误
- [ ] `allowlist` 下非白名单 model 被拒绝
  Expected result: 返回 `model_not_allowlisted` 或等价策略错误
- [ ] `POST /api/ask` 在 runtime policy violation 时先被阻断
  Expected result: ask 在真正调用模型前返回 forbidden / policy message
- [ ] `policy_violation` 写入审计且不泄露 API key
  Expected result: `${CONFIG_DIR}/Memori-Vault/audit.log.jsonl` 有 `policy_violation` 事件，但不包含明文密钥

### Desktop smoke checklist

- [ ] 设置页可读取与保存 enterprise policy
  Expected result: `get_enterprise_policy` / `set_enterprise_policy` 往返字段完整
- [ ] `local_only` 下远端配置仍可编辑，但不能成为 active runtime
  Expected result: 保存远端配置失败，UI 展示明确 policy error
- [ ] `local_only` 下 probe / list 被策略阻断
  Expected result: Settings 中远端 provider 探测和模型列表刷新失败，并显示企业策略阻断原因
- [ ] `allowlist` 下白名单 endpoint/model 可通过
  Expected result: 允许的 endpoint 与 model 可保存；若本机模型服务缺失，则标记环境阻塞而非策略失败
- [ ] `allowlist` 下非白名单 endpoint/model 被拒绝
  Expected result: UI 保持可编辑，但 active runtime 无法切换到非法远端配置
- [ ] 切回本地 provider 后 ask / indexing 恢复
  Expected result: 本地 provider 重新成为 active runtime，结构化 ask 与索引流程可继续工作

### Environment notes

- 若本机缺少 Ollama 或缺少所需本地模型，请标记为 `environment blocked`
- 不允许用远端 provider 替代本轮企业本地优先验收

## 6. Release Workflow

- [ ] `desktop-release.yml` 已校验 tag/version 一致
- [ ] `desktop-release.yml` 已校验 release notes 文件存在
- [ ] draft release 使用正式 `docs/RELEASE_NOTES_vX.Y.Z.md`
- [ ] 三端构建产物上传规则与当前 Tauri 输出一致

## 7. 发版前手动检查

- [ ] Windows 包可安装并启动
- [ ] Linux 包可启动并加载 UI
- [ ] macOS 包可启动并加载 UI
- [ ] 首次启动时基础流程清晰：选择目录、配置模型、发起首次检索
- [ ] About / 设置页显示的版本号与 release 版本一致

## 8. 发版后动作

- [ ] 检查 GitHub draft release 附件是否齐全
- [ ] 检查 release title、tag、notes 是否匹配
- [ ] 发布后验证下载链接可用
- [ ] 在 README 或官网入口同步最新版本说明（如适用）

## 建议发布口径

- 个人版：可作为当前主要发布目标
- 服务端 / 私有化：建议以 preview 口径发布
- 企业能力：建议明确为 private deployment preview，而不是完整 GA 企业版
- 检索质量：建议明确写成“当前 citations 可信，但 mixed corpus document routing 仍在持续验证”，不要写成已经完成大规模高精度验证
