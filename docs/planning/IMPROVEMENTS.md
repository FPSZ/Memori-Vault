# Memori-Vault 改进清单（按优先级）

> **本轮重写：2026-06-10。** 取代 2026-06-02/05 的旧清单——彼时多条 P0/P1 已落地，旧口径（v1 小语料、`repo_mixed`、OIDC 提权）已不反映现实。本文件现为**当前真实改进 backlog**，下个阶段主攻 **工程硬化 / 产品化**。
>
> 检索质量口径已迁移到 v2 困难基准：`docs/qa/RETRIEVAL_BASELINE_V2.md`（548 文档 / 126 题）。本清单只记录"待改进"，不重复已做好的部分。

优先级：**P0** 阻塞可信对外交付 ／ **P1** 近期高价值低风险 ／ **P2** 产品化·可运维 ／ **P3** 大演进。
类别：`[安全]` `[工程]` `[检索]` `[文档]` `[体验]`

> **全项目体检**见 `docs/planning/PROJECT_AUDIT_2026-06-10.md`（跨 9 个域、带 `file:line` 证据与严重度）。下方为该审计提炼出的可执行 backlog。

---

## ✅ 已解决（旧清单遗留，本轮核实关闭）

| 旧编号 | 项 | 现状（已验证） |
| --- | --- | --- |
| P0-1 `[安全]` | OIDC 登录未验签 / body 直传 `role` 提权 | **已修**：`routes/auth.rs` 默认走 `verify_oidc_token_claims`（JWKS + iss/aud/exp）；免验签 `decode_jwt_claims` 仅在 `allow_insecure_oidc_dev_login()` dev 开关下可用；body 直接指定 `role` 的能力已删，role 来自已验证 claims。 |
| P0-2 `[检索]` | `repo_mixed` Top-1 退步到 0.4773 | **口径作废**：v1 小语料已退役，改用 v2 困难基准。当前 top3 文档召回 **0.925**、片段 MRR **0.839**、reject **0.881**、citation **1.000**。 |
| P1-7 `[工程]` | 上帝文件待拆 | **基本完成**：`memori-core/lib.rs` 2388→**776**、`retrieval.rs` 1750→**745**、`memori-storage/lib.rs` 1548→**595**、`ui/App.tsx` 1858→**801**、`mcp/tools.rs` 1155→**239**。 |
| P2-8 `[安全]` | storage/schema 层 `unwrap/expect/panic` | **已清**：`memori-storage/src/lib.rs` + `schema.rs` 非测试代码 0 处。 |
| 8.2 `[安全]` | CORS `allow_origin(Any)` | **已收敛**：`build_cors_layer` 的 `allow_origin = resolve_allowed_origins()`，默认仅 localhost/tauri 白名单，可经 `MEMORI_ALLOWED_ORIGINS` 覆盖（残留 methods/headers=Any，见 E3）。 |
| — `[工程]` | 图谱构建过慢（2.5 万字 ≈33 分钟） | **已提速 4–5×**：精简 schema + 约束/封顶解码 + 并发 2→4，长文降到 ~7 分钟量级（见 baseline v2「图谱构建速度优化」）。 |

---

## P1 — 工程硬化（近期高价值、低风险，下阶段主线）

### E1 `[工程]` 合并重复 model-helper 模块
`ui/src/components/settings/tabs/` 下两份近重复：`modelUtils.ts`（13.5KB，LIVE，`ModelsTab` 引用）与 `models-helpers.ts`（8KB，仅 `ModelCard` 引少量）。两份 `ROLE_META`/校验逻辑漂移风险高。
**做法**：统一到 `modelUtils.ts`，改 `ModelCard` 引用，删 `models-helpers.ts`，`tsc --noEmit` 校验。
**验收**：单一事实源，UI 行为不变，编译干净。

### E2 `[安全]` 会话管理补全
`memori-server` 只有 8h `DEFAULT_SESSION_TTL_SECS`，**无主动登出 / 失效端点**，无最长会话上限，无防重放。
**做法**：补 `POST /auth/logout`（删 session）、显式失效、最长会话上限、最小防重放约束；补测试。
**验收**：登出后旧 token 立即失效；超龄会话被拒。

### E3 `[安全]` CORS 收尾
`build_cors_layer` 的 `allow_methods(Any)` / `allow_headers(Any)` 仍宽。
**做法**：收成实际用到的方法（GET/POST/OPTIONS）与头部白名单。
**验收**：预检只放行声明的方法/头部。

### E12 `[体验/工程]` React ErrorBoundary（审计 F1，建议优先）
前端**无 ErrorBoundary**，任一渲染异常会白屏整个 App。
**做法**：在 App 根包一个 ErrorBoundary，捕获后降级到可读错误页 + 重载入口。成本极低、收益明显。

### E13 `[工程]` CI 跨平台矩阵 + 依赖漏洞扫描（审计 C1/C2）
`rust-ci.yml` 仅 `ubuntu-latest`，主用户在 Windows 且有已知 GBK 问题；无 `cargo-deny/audit`、无 `npm audit`。
**做法**：PR 检查加 windows/macos 矩阵；加依赖漏洞扫描 step；借机在 Windows 上复核旧 GBK 错误串乱码（审计 C3）。

### E14 `[安全]` 密钥不落明文 + 审计不静默丢失（审计 S1/S5）
远程 API Key 明文存 `settings.json`；审计写失败是 warn-and-drop。
**做法**：密钥接 OS keychain（keyring）；审计 IO 失败升级为告警/重试，避免静默丢审计。

### E4 `[工程/检索]` 回归 CI 守门
现 CI（`.github/workflows/rust-ci.yml`）只跑 `fmt` + `clippy -D warnings` + `cargo test --workspace`，**无检索质量阈值门**——指标静默退步无人拦。live 档需本地模型不便上 CI。
**做法**：加一档**可在 CI 跑的确定性/离线 embedding 回归**，对 `top3_doc / chunk_mrr / reject_correct` 设阈值，低于版本化基线即红灯；live 档继续本地手动并把 baseline 数字入库 diff。
**验收**：任何使关键指标低于阈值的提交 CI 红灯。

---

## P2 — 产品化 / 可运维

### E5 `[工程]` memori-server OpenAPI spec
当前 server **无 OpenAPI**（无 utoipa/swagger）。UI 与第三方靠读代码对接。
**做法**：用 `utoipa` 为 HTTP API 标注生成 spec + Swagger UI，API 版本化。
**验收**：`/api-docs/openapi.json` 可用，覆盖现有路由。

### E6 `[安全/工程]` 管理接口限流 + 链路 trace
**无限流**（无 governor/tower::limit）；检索链路无 request-id。
**做法**：给 `auth`/admin 接口加最小限流；贯穿 request-id 到 `query analysis → doc routing → chunk merge → answer`，落最小可观测 trace。
**验收**：暴力登录被限；一条 request-id 可串起整链路耗时。

### E7 `[工程]` 50k 规模本地压测
扩展性目标态是 50k 文档，但**从未做过规模压测**，SQLite 单 `Mutex<Connection>` 是否成瓶颈未知。
**做法**：干扰库扩到 ~50k，记录 `doc_recall_ms/chunk_recall_ms/merge_ms/answer_ms` 的 P50/P95；形成"连接模型是否需升级（读写分离/连接池/ANN）"决策文档（含 benchmark 证据）。
**验收**：有规模化延迟数据 + 明确扩展决策。

### E8 `[文档]` README 成熟度对齐
README 仍可能让外部读者高估成熟度（叙事盖过实测）。
**做法**：用 ✅已验证 / 🚧进行中 / 📐设计 状态徽标区分能力；明确 preview 信任模型与版本语义（"工程骨架冻结"而非"质量达标"）。
**验收**：每条"核心能力"都有清晰状态标注。

---

## P3 — 大演进（需求确认后再排期）

- **E9 `[工程]` 多租户 / 多资料库隔离**：当前 OIDC 登录后**共享同一个库**，无 `workspace_id/tenant/user_id` 隔离。设计 DB / 索引 / 审计三层隔离边界。（本地优先产品是否真要多租户，需先确认需求。）
- **E10 `[体验]` 增量索引进度推送（SSE/WebSocket）**：长任务索引当前无实时进度通道。
- **E11 `[工程]` shell-service 共享层**：收敛 desktop/server 在 `settings/model-policy/auth/audit` 的重复流程，统一 DTO 与错误码映射，建能力矩阵（desktop-only / server-only / shared）。

---

## 检索/作答质量 backlog（非本阶段主线，记录留档）

> 下阶段选定了工程硬化方向；以下质量杠杆来自 v2 失败分析，**待后续单独成轮**。详见 `RETRIEVAL_BASELINE_V2.md`「下一步杠杆」。

1. **A 类 gating 误拒**：证据已召回却被阈值 55 砍（`多格式抽取 1/6`、`长文 0/2`）——按 coverage / rerank 置信度调放行路径。最高 ROI。
2. **长文深埋事实**（`V101/V102` 0/2）：Parent Document Expansion（高分 chunk 拉同文档上下文，上限 8000 字符）。
3. **跨语言 V119**：中文问→英文邮件埋点例外漏召——双语 query 扩展 / 别名映射。
4. **B 类诱饵代号拒答残留**（`V086/V087/V092`）：需语义级代号核验，有误伤 answer 风险，谨慎。
5. **OCR**：图片/扫描件 0/4 不可检索——大功能，接 tesseract 或视觉模型。
6. **作答层评测盲区**：harness 只用 top-k 当代理，不给 LLM 答案文本判分——接 LLM-judge 闭环事实正确性/忠实度。

---

> 一句话：**核心检索与安全硬伤已基本收口，骨架是优等生；下阶段把"可对外交付"的工程/产品化短板（会话/限流/OpenAPI/CI 守门/规模压测）补齐，让成熟度叙事真正配得上实测。**
