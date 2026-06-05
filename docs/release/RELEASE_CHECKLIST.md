# Memori-Vault 发布检查清单

这份清单用于桌面版发布前的最终确认，目标是把“能构建”提升为“可以对外发布”。如果某项没有验证，必须保持未勾选，不要用描述性文案替代实测结果。

## Memory OS Lite 发布门槛

- [ ] README 使用产品定位语言，而不是变更日志语言：说明问题、为什么不是普通 RAG 工具、核心优势、快速开始、架构和当前边界。
- [ ] [MEMORY_OS_LITE.md](../architecture/MEMORY_OS_LITE.md) 反映当前实现状态和待办项。
- [ ] Trust Panel 展示 `answer_source_mix`、`failure_class`、`source_groups`、`memory_context`、`context_budget_report`。
- [ ] Evidence Firewall 已验证：文档引用只能来自 document chunks。
- [ ] MCP `tools/list` 包含 query/source 工具和 memory 工具。
- [ ] `memory_add` / `memory_update` 写入必须绑定 source，或被策略拒绝，并产出 lifecycle/audit 记录。
- [ ] 50-case acceptance report 已附上，或明确标记 pending。
- [ ] Release notes 不把 temporal graph、Markdown source-of-truth、heat score、50k-document validation 写成已完成，除非有实测证据。

## 1. 版本与文档

- [ ] `Cargo.toml` 的 `workspace.package.version` 已更新。
- [ ] `ui/package.json` 的 `version` 已更新。
- [ ] `memori-desktop/tauri.conf.json` 的 `version` 已更新。
- [ ] 以上三个版本号保持一致。
- [ ] 已编写对应版本的 release notes：`docs/release/RELEASE_NOTES_vX.Y.Z.md`。
- [ ] `README.md` 和 `README.en.md` 的运行模式、企业版口径、检索边界已经同步。
- [ ] `docs/guides/enterprise.md` 和 `docs/guides/enterprise.zh-CN.md` 的 preview/GA 口径已经同步。
- [ ] `docs/qa/RETRIEVAL_BASELINE.md` 已更新到最新可复现数据。

## 2. 代码质量门槛

- [ ] 运行 `cargo fmt --all -- --check`。
- [ ] 运行 `cargo clippy --workspace -- -D warnings`。
- [ ] 运行 `cargo test --workspace`。
- [ ] 运行 `pnpm --dir ui install --frozen-lockfile`。
- [ ] 运行 `pnpm --dir ui run build`。
- [ ] CI（`rust-ci.yml`）已通过。
- [ ] 没有未解释的 `panic`、`unwrap`、`expect` 被引入到核心运行路径。

## 3. 索引与检索行为验证

- [ ] 新增或修改 `.md` / `.txt` / `.docx` / `.pdf` 文件后，可正常进入索引。
- [ ] 删除单个文档后，旧内容不会继续出现在检索结果中。
- [ ] 将支持的文件重命名为不支持的后缀后，旧索引会被清理。
- [ ] 删除目录后，目录内旧索引会被清理。
- [ ] 当 parser / index 语义版本不兼容时，系统会自动进入 `required` / `rebuilding`。
- [ ] 在 `required` / `rebuilding` 期间，search / ask 会显式拒绝，而不是继续读取旧索引。
- [ ] 全量重建完成后，`rebuild_state` 会恢复为 `ready`。
- [ ] 索引状态不会卡在 `scanning` / `embedding`。
- [ ] 设置中心可正常保存关键配置。
- [ ] `core_docs` / `full_live` 回归使用 suite target documents，不应把整个仓库、`target/`、`.git/`、`node_modules/` 全量喂给 live embedding。

## 4. 运行时与产品口径

- [ ] 桌面版作为当前主要体验进行发布。
- [ ] `memori-server` 口径保持为 server runtime / private deployment preview。
- [ ] 企业能力口径保持为 preview，不对外宣称完整 GA 级企业身份安全能力。
- [ ] llama.cpp 依赖、推荐模型、端口配置说明清楚。
- [ ] 本地模型不可用时，UI 明确提示模型未配置或服务不可达，不自动降级到未授权远程模型。
- [ ] rerank 模型不可用时，检索应自动降级到非 rerank 排序，并在报告里记录 `rerank_health=disabled/unavailable`。

## 5. 自动化证据快照

### 2026-03-11 UTC

- `cargo test -p memori-core --lib` 已通过。
- `cargo check -p memori-core -p memori-desktop -p memori-server` 已通过。
- `pnpm --dir ui run build` 已通过。
- offline regression 已跑通：
  - `offline_deterministic + core_docs`
  - `offline_deterministic + repo_mixed`
- 报告路径：
  - `target/retrieval-regression/offline_deterministic-core_docs-1773229611/report.json`
  - `target/retrieval-regression/offline_deterministic-repo_mixed-1773229598/report.json`
- 当时结果：
  - `core_docs`: `Top-1=0.6970`, `Top-3=0.6970`, `Top-5=0.7576`, `citation validity=1.0`, `reject correctness=1.0`
  - `repo_mixed`: `Top-1=0.4773`, `Top-3=0.4773`, `Top-5=0.5455`, `citation validity=1.0`, `reject correctness=0.94`
- 当时 live regression 状态：
  - `live_embedding + full_live`
  - blocked by local llama.cpp / embedding endpoint availability

### 2026-06-05 UTC

- live regression 已端到端跑通：
  - mode/profile: `live_embedding + full_live`
  - suite: `docs/qa/retrieval_regression_suite.json`
  - report JSON: `target/retrieval-regression/live_embedding-full_live-1780648792/report.json`
  - report Markdown: `target/retrieval-regression/live_embedding-full_live-1780648792/report.md`
  - local services: `service_health=ready`, `rerank_health=ready`
  - index preparation: `80,251 ms`
  - indexed scope: suite target documents only, not full repository
  - indexed corpus: `100` documents / `801` chunks
- measured metrics:
  - cases: `100`
  - passed / failed: `91 / 9`
  - answer / refuse cases: `88 / 12`
  - answer cases answered correctly: `80 / 88`
  - refuse cases refused correctly: `11 / 12`
  - Top-1 document hit: `87.50%`
  - Top-3 document recall: `93.18%`
  - Top-1 chunk hit: `81.82%`
  - Top-5 chunk recall: `93.18%`
  - Chunk MRR: `0.8561`
  - citation validity: `100.00%`
  - reject correctness: `91.00%`
  - rerank applied: `95.00%`
- 结论：
  - local live retrieval path 已经可运行，不再被 embedding/rerank 服务缺失阻塞。
  - 检索质量仍低于 release-quality claims。
  - 最高优先级 blocker 是中文事实卡直问（`4/20`）和拒答安全/意图处理（越权/注入/常识类 `1/6`）。
  - current residual recalled-document gating false refusals: `6`; continue tracking them as targeted residuals rather than a broad gate failure.

## 6. 当前发布口径

- 企业本地优先运行时和策略阻断链路已有实现与文档闭环。
- 检索质量和企业策略是两条不同验收线，不能互相替代。
- Citation validity 表现稳定，但 mixed corpus / Memory_Test 的真实检索精度仍未达高精度发布口径。
- 如果当前发布，检索能力应写作 internal preview / beta，不应宣称“大规模高精度知识检索已完成”。
- `live_embedding + full_live` latest 100-case result is `91/100`; release wording may claim this regression corpus result, but must not generalize it to 50k-document high-accuracy validation.

## 7. Server API 检查

- [ ] `GET /api/admin/policy` 可读取当前 enterprise policy。
  Expected result: 返回 `egress_mode`、`allowed_model_endpoints`、`allowed_models`，且默认值符合 `local_only`。
- [ ] `PUT /api/admin/policy` 更新后立即触发 engine re-evaluation。
  Expected result: policy 更新后新的 ask / model runtime 使用新策略，不继续沿用旧 runtime。
- [ ] `local_only` 下保存远端 runtime 被拒绝。
  Expected result: `POST /api/model-settings` 返回明确 forbidden / policy message。
- [ ] `local_only` 下远端 probe / list 被拒绝。
  Expected result: `POST /api/model-settings/probe` 和 `POST /api/model-settings/list-models` 返回明确策略阻断，而不是普通网络错误。
- [ ] `allowlist` 下非白名单 endpoint 被拒绝。
  Expected result: 返回 `remote_endpoint_not_allowlisted` 或等价策略错误。
- [ ] `allowlist` 下非白名单 model 被拒绝。
  Expected result: 返回 `model_not_allowlisted` 或等价策略错误。
- [ ] `POST /api/ask` 在 runtime policy violation 时先被阻断。
  Expected result: ask 在真正调用模型前返回 forbidden / policy message。
- [ ] `policy_violation` 写入审计且不泄露 API key。
  Expected result: `${CONFIG_DIR}/Memori-Vault/audit.log.jsonl` 有 `policy_violation` 事件，但不包含明文密钥。

## 8. Desktop smoke 检查

- [ ] 设置页可读取与保存 enterprise policy。
  Expected result: `get_enterprise_policy` / `set_enterprise_policy` 往返字段完整。
- [ ] `local_only` 下远端配置仍可编辑，但不能成为 active runtime。
  Expected result: 保存远端配置失败，UI 展示明确 policy error。
- [ ] `local_only` 下 probe / list 被策略阻断。
  Expected result: Settings 中远端 provider 探测和模型列表刷新失败，并显示企业策略阻断原因。
- [ ] `allowlist` 下白名单 endpoint/model 可通过。
  Expected result: 允许的 endpoint 和 model 可保存；若本机模型服务缺失，则标记环境阻塞而非策略失败。
- [ ] `allowlist` 下非白名单 endpoint/model 被拒绝。
  Expected result: UI 保持可编辑，但 active runtime 无法切换到非法远端配置。
- [ ] 切回本地 provider 后 ask / indexing 恢复。
  Expected result: 本地 provider 重新成为 active runtime，结构化 ask 与索引流程可继续工作。

## 9. 环境说明

- [ ] 若本机缺少 llama.cpp 或所需本地模型，记录为 `environment blocked`。
- [ ] 不允许用远端 provider 替代本轮企业本地优先验收。
- [ ] 本地模型端口记录清楚：chat / graph / embed / rerank。
- [ ] rerank 开启、关闭、不可用三种状态均可在报告中区分。

## 10. Release workflow

- [ ] `desktop-release.yml` 已校验 tag/version 一致。
- [ ] `desktop-release.yml` 已校验 release notes 文件存在。
- [ ] draft release 使用正式 `docs/release/RELEASE_NOTES_vX.Y.Z.md`。
- [ ] Windows / Linux / macOS 构建产物上传规则与当前 Tauri 输出一致。

## 11. 发布前手动检查

- [ ] Windows 包可安装并启动。
- [ ] Linux 包可启动并加载 UI。
- [ ] macOS 包可启动并加载 UI。
- [ ] 首次启动基础流程清楚：选择目录、配置模型、建立索引、发起首次检索。
- [ ] About / 设置页显示的版本号与 release 版本一致。
- [ ] 中文界面文案无乱码。
- [ ] 回归报告 Markdown 无乱码。

## 12. 发布后动作

- [ ] 检查 GitHub draft release 附件是否齐全。
- [ ] 检查 release title、tag、notes 是否匹配。
- [ ] 发布后验证下载链接可用。
- [ ] 在 README 或官网入口同步最新版本说明（如适用）。

## 建议发布文案

- 个人版：可作为当前主要发布目标。
- 服务端 / 私有化：建议使用 preview 口径发布。
- 企业能力：建议明确为 private deployment preview，而不是完整 GA 企业版。
- 检索质量：建议写成“citations 可信，mixed corpus document routing 仍在持续验证”，不要写成“已完成大规模高精度验证”。
