# Memori-Vault 改进清单（按优先级）

## 2026-06-04 最新检索实测更新

本次更新来自 100 条 `Memory_Test/` live 回归：

- 运行模式：`live_embedding + full_live`
- 报告：`target/retrieval-regression/live_embedding-full_live-1780575982/report.json`
- 本地服务：`service_health=ready`，`rerank_health=ready`
- 索引准备：`44,226 ms`
- 索引规模：`54` documents / `425` chunks
- 用例：`100`，其中 `88 answer / 12 refuse`
- 总体通过：`56/100`
- Top-1 document hit：`35.23%`
- Top-3 document recall：`59.09%`
- Top-1 chunk hit：`54.55%`
- Top-5 chunk recall：`69.32%`
- Chunk MRR：`0.5987`
- Citation validity：`100.00%`
- Reject correctness：`48.00%`
- Rerank applied：`66.00%`

按能力维度看，当前最差的不是多格式解析，而是事实卡直问和拒答安全：

| 能力 | 通过 / 总数 | 通过率 |
| --- | ---: | ---: |
| 文档类型定位 | 5 / 5 | 100.00% |
| 反常识/抗参数知识 | 9 / 10 | 90.00% |
| 多格式抽取 | 6 / 7 | 85.71% |
| 口语/错别字/省略鲁棒 | 4 / 5 | 80.00% |
| 长难句/多条件 | 4 / 5 | 80.00% |
| 跨文档综合(2-3 文档) | 6 / 8 | 75.00% |
| 相似代号防串 | 3 / 5 | 60.00% |
| 改写/语义召回 | 9 / 16 | 56.25% |
| refuse-库中无此事实 | 3 / 6 | 50.00% |
| 代号/别名/ID 检索 | 2 / 7 | 28.57% |
| 中文直问-事实卡命中 | 4 / 20 | 20.00% |
| refuse-越权/注入/常识 | 1 / 6 | 16.67% |

新的 P0 结论：

- `中文直问-事实卡命中` 只有 `4/20`，这是最高优先级精度缺陷；这类问题理论上应最容易命中。
- `refuse-越权/注入/常识` 只有 `1/6`，拒答安全/意图识别需要单独修。
- `score_below_threshold` 出现 `42` 次，是最大失败归因；下一轮要区分“正确 chunk 已召回但 gating 误拒”和“根本没召回”。
- `citation_validity=100%` 说明引用链可信，但排序/门控还不够好，不能对外宣称高精度。

> 生成日期：2026-06-02
> 评审范围：当前 `main` 分支代码（Rust workspace ~28k 行 + UI ~10k 行）、文档、CI、QA baseline。
> 说明：本清单只记录"需要改进的地方"，不重复已经做得好的部分（干净的错误处理、CI 已含 `clippy -D warnings`/`fmt`/`test`、53 条回归用例、诚实的 retrieval baseline 等）。

优先级定义：

- **P0** — 阻塞"可信 / 可对外声称 1.0"的硬伤，应最先处理。
- **P1** — 影响核心价值与长期可维护性，近期内必须推进。
- **P2** — 健壮性、安全细节、体验，价值高但不阻塞。
- **P3** — 路线图、打磨、长期项。

类别标签：`[安全]` `[检索]` `[工程]` `[文档]` `[体验]`

---

## P0 — 必须最先修

### P0-1 `[安全]` OIDC 登录未验签，存在管理员提权漏洞

**问题**
`memori-server/src/routes/auth.rs:13-44` 的 `oidc_login_handler`：

1. 直接从请求体取 `id_token` / `access_token`，调用 `decode_jwt_claims`（`memori-server/src/auth.rs:30-47`）。该函数**只做 base64 解码 payload，完全不验证签名、issuer、audience、exp**。
2. 从这些**未验证**的 claims 中取 `sub` / `email` 作为身份，取 `roles_claim` 作为角色，并签发真实会话。
3. 更直接：`payload.role`（请求体字段）可直接覆盖角色 —— 客户端只要 POST `{"subject":"x","role":"admin"}` 就能拿到 admin 会话。

**影响**：任何能访问 `/auth/oidc` 的人都能伪造任意身份与角色，整个 RBAC / audit 体系形同虚设。这与 README 宣称的"企业私有化 preview：RBAC、audit、egress policy"直接冲突。

**修复方向**
- 用 `jsonwebtoken` crate 按 IdP 的 JWKS 验证签名 + `iss` + `aud` + `exp`/`nbf`。
- 删除或严格限制请求体中可直接指定 `role` 的能力（至少在非 dev 模式下禁止）。
- 在文档里明确：当前 preview 的信任模型是什么、未配置 IdP 时的默认拒绝行为。
- 加一条测试：伪造 JWT / 直传 `role=admin` 必须被拒。

**验收**：伪造未签名 token 或直传 `role` 无法获得高于 `Viewer` 的权限。

---

### P0-2 `[检索]` 混合语料文档路由精度不达标（核心价值短板）

**问题**（来源：`docs/qa/RETRIEVAL_BASELINE.md`）
- `repo_mixed`（11 篇文档）最新 `Top-1 = 0.4773`，即正确文档排第一的概率不到一半。
- 且**相对上一快照退步**：`Top-1 0.5682 → 0.4773`、`Reject 0.98 → 0.94`。
- 主导失败模式有据可查：
  - 英文描述型查询路由到错误文档：`R02 R05 R13 R21 R28 R35 R36`
  - 代码 / 实现型查询正确文件被压低：`R40 R42 R43 R44 R45 R46 R50 R51`

文档级合并权重逻辑集中在 `memori-core/src/retrieval.rs`（`merge_document_candidates`，已按 `QueryFamily` 动态调权），是调优主战场。

**这是产品的核心卖点**——"问你的文档，知道答案从哪里来"。检索排不准，可验证性再好也只是"能信任它什么时候答，但答得不一定对"。

**修复方向**
- 先定位 `Top-1` 退步的提交（用回归套件二分），止血回归再谈提升。
- 针对两类失败分别调：描述型英文 query 的 `documents_fts` 路由权重 vs. 实现型 query 的代码文档权重。
- 把"修一类不破坏另一类"做成回归约束（见 P1-4 CI 守门）。

**验收**：`repo_mixed` 至少恢复到 `Top-1 ≥ 0.57`，并向 README 写的目标（`answered ≥ 45/50`、`correct ≥ 40/50`、`citation/source_group_hit ≥ 45/50`）推进。

---

### P0-3 `[文档]` 版本号 / 对外声称 与实测质量不一致

**问题**
- 仓库已打 `v1.0.0`，但 `RETRIEVAL_BASELINE.md` 自己写明 `repo_mixed` 仍是 "beta/internal-only quality"、"not ready for strong accuracy claims"。
- README "核心优势"读起来像分层记忆/图谱/heat score 等都已就绪，但"当前状态"里多数标注为"部分落地 / 仍在推进"。

**影响**：愿景叙事盖过实测现实，外部读者会高估成熟度，反噬信任——而信任正是本项目的立身之本。

**修复方向**
- README 中把"已验证可用"与"设计中/部分落地"用硬性分区或状态徽标区分（例如 ✅ 已验证 / 🚧 进行中 / 📐 设计）。
- 明确 1.0.0 的语义：是"工程骨架冻结"还是"质量达标"。建议表述为前者。

---

## P1 — 近期必须推进

### P1-4 `[检索/工程]` 回归指标缺 CI 守门，导致精度静默退步
**问题**：`Top-1` 在两次快照间退步且无人拦截，说明回归套件目前是"手动跑、手动看"。
**建议**：把 `retrieval_regression` example 接入 CI（或定时任务），对 `core_docs` / `repo_mixed` 设阈值，低于基线即 fail 或告警；基线值入库版本化。
**验收**：任何使 `Top-1` 低于阈值的提交在 CI 红灯。

### P1-5 `[检索]` 回归语料过小，指标统计意义弱
**问题**：`core_docs=6` 篇、`repo_mixed=11` 篇文档。11 篇语料上 `Top-1` 的单条波动就能造成数个百分点抖动。
**建议**：构建一个更大、更有代表性的混合语料（中文/繁体/英文/代码/成对重复 `.txt/.md`），向"50 条验收 + 真实规模"靠拢；同时保留小语料做快速回归。

### P1-6 `[检索]` Gating 误杀（可答问题被拒答）
**问题**：`R19 R35 R42` 等可答问题在 `repo_mixed` 被 gating 拒答（`Reject` 从 0.98 退到 0.94 也印证）。
**位置**：`memori-core/src/engine.rs`（`apply_gating_metrics` / `evaluate_gating_decision` / `answer_indicates_insufficient_evidence`）。
**建议**：把 gating 误杀单列为一个失败类指标跟踪，调阈值时同时盯 false-negative 与 citation 准确率，避免按下葫芦浮起瓢。

### P1-7 `[工程]` 上帝文件待拆分
**问题**：
- `memori-core/src/lib.rs` 2388 行：把 ~30 个 `pub struct`/`enum`、`AppState`、策略解析、env 解析全堆在一个模块。
- `ui/src/App.tsx` 1858 行（UI 已开始拆分到 `ui/src/app/{panels,hooks,layout}`，但主文件仍臃肿）。
- `memori-core/src/retrieval.rs` 1750、`memori-storage/src/lib.rs` 1548、`memori-server/src/mcp/tools.rs` 1155。
**建议**：延续已有重构方向。优先把 `lib.rs` 的类型定义拆成 `types.rs` / `policy.rs`；把 `App.tsx` 的 state 与 effect 继续抽进 hooks。这是 README "当前状态"里已自认的待办，落实即可。

---

## P2 — 健壮性 / 安全细节 / 体验

### P2-8 `[安全]` storage 层 `unwrap/expect/panic` 运行时风险
**问题**：`src` 主路径整体很干净（业务代码 0 `unwrap`），但 `memori-storage/src/lib.rs`、`schema.rs` 与 `memori-core/src/lib.rs` 中仍有 `unwrap/expect/panic`（examples 里的可忽略）。schema/存储层一旦 panic 会拖垮整个服务。
**建议**：审计这几处，DB 初始化 / 迁移失败应返回 `Result` 并优雅降级，而非 panic。

### P2-9 `[体验/工程]` 错误信息编码错乱（Windows GBK→UTF-8 乱码）
**问题**：`RETRIEVAL_BASELINE.md:136` 出现 `Embedding 璇锋眰澶辫触: ...` —— 这是 GBK 字节"请求失败"被当 UTF-8 解读的典型乱码。说明在中文 Windows 上捕获子进程/系统错误串时未按 GBK 正确解码。
**建议**：捕获外部错误文本时显式按系统码页解码或统一转 UTF-8，避免日志/Trust Panel 里出现乱码。

### P2-10 `[检索]` live embedding 基线已跑通，但真实精度未达标
**更新**：2026-06-04 已完成 `live_embedding + full_live` 100 条端到端回归，`service_health=ready`，`rerank_health=ready`，报告见 `target/retrieval-regression/live_embedding-full_live-1780575982/report.json`。
**问题**：服务可用性不再是 blocker，但真实精度仍不达标：`56/100` 通过，`Top-1 document hit=35.23%`，`Top-3 document recall=59.09%`，`Top-5 chunk recall=69.32%`，`reject correctness=48.00%`。
**建议**：下一步不要再把问题归为“模型没启动”；应拆分 `retrieval_miss / gating_false_refusal / refuse_policy_miss`，优先修 `中文直问-事实卡命中=4/20` 与 `refuse-越权/注入/常识=1/6`。

### P2-11 `[工程]` PDF/DOCX/HTML 摄入稳定化
**问题**：README 自列为"仍在推进"。`memori-parser` 仅 738 行单文件，多格式稳健解析（编码、表格、扫描件、HTML 噪声）容易出问题。
**建议**：为各格式补固定样本 + 解析回归用例，把摄入失败率纳入指标。

---

## P3 — 路线图 / 打磨

### P3-12 `[文档]` Memory OS Lite 高级能力区分"已落地 vs 设计中"
heat score、conflict resolver、lifecycle classifier、temporal graph explanation 等在 README/架构文档里描述完整，但多为设计或部分落地。建议在 `docs/architecture/MEMORY_OS_LITE.md` 用状态标注，避免读者误判。

### P3-13 `[工程]` `memori-vault` crate 定位
`memori-vault` 仅 423 行单文件，作为被 core 依赖的基础 crate，职责边界值得在 `docs/architecture/STRUCTURE.md` 里写清，避免与 `memori-core` 概念重叠。

### P3-14 `[体验]` i18n 与 onboarding 完整度
UI 有 `i18n.tsx` 与 `OnboardingOverlay`，README 强调中文友好。建议系统性核对中英文案覆盖率与首次启动引导是否覆盖"配本地模型 → 建索引 → 提问看 Trust Panel"主链路。

---

## 建议执行顺序（摘要）

1. **P0-1 安全提权** — 一两天即可堵住，风险/成本比最高，先做。
2. **P0-2 + P1-4 检索精度止血 + CI 守门** — 先二分定位退步提交，配 CI 阈值防再退。
3. **P0-3 文档对齐** — 低成本、立刻提升可信度。
4. **P1-5/6/7** — 扩语料、调 gating、拆大文件，三者并行推进检索质量与可维护性。
5. **P2/P3** — 随迭代消化。

> 一句话：**概念与工程骨架已是优等生，瓶颈在"核心检索精度"和"一个具体的安全硬伤"。把这两点解决，1.0 的成色才真正配得上叙事。**
