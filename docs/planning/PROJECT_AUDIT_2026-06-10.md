# Memori-Vault 全项目审计（2026-06-10）

> 一次跨全部模块的体检：security / 检索质量 / 扩展性 / 工程 / 前端 / CI / 产品化 / 文档。每条带证据（`file:line`）与严重度，并标注处置（已在 backlog / 新增 / 留档）。
>
> 严重度：🔴 高（阻塞对外交付或有真实风险）｜🟡 中（应近期处理）｜🟢 低（defense-in-depth / 打磨）｜✅ 现状良好（不用动，列出以明确基线）。
> backlog 编号 `E*` 见 `IMPROVEMENTS.md` / `plan.md` Phase 9；质量项见 `RETRIEVAL_BASELINE_V2.md`。

## ✅ 本轮已修（2026-06-10，安全/工程速赢批 + CI 矩阵批）

| 项 | 处置 | commit |
|---|---|---|
| D1 重复 model-helper | 删 `models-helpers.ts`，唯一引用方 ModelCard 改指 modelUtils；顺修漏 18004 的端口报错 | `c152872` |
| F1 无 ErrorBoundary | 顶层 `ErrorBoundary` 包在 I18nProvider 外，渲染异常降级为错误页+重载，不再白屏 | `75ce914` |
| S4 CORS methods/headers=Any | 收成 GET/POST/PUT/OPTIONS + Authorization/Content-Type | `f2e298a` |
| S3 无会话失效 | 新增 `POST /api/auth/logout`(幂等+审计) + `enforce_session_cap` 活跃会话上限 2048(纯函数+单测) | `c32aa6d` |
| C1 CI 仅 ubuntu | rust-ci.yml 拆 4 job；`clippy-test` 矩阵 ubuntu/windows/macos，Linux 条件装 apt deps | `c6aa666` |
| C2 无依赖漏洞扫描 | 新增 `security` job：`EmbarkStudios/cargo-deny-action` check advisories+bans+sources + `pnpm audit --audit-level high`；新增 `deny.toml` | `c6aa666` |
| S1 API key 明文存 JSON | `keychain.rs`：OS keychain 存储（Windows Credential Manager / macOS Keychain / Linux Secret Service）；JSON 只存哨兵 `__keychain__`；写 keychain 失败时降级明文+warn；读时透明替换 | 本次 |
| S5 审计写失败静默丢弃 | `audit.rs` 所有 IO 失败从 `warn!` 升级为 `error!`（含 path 解析/目录创建/文件打开/写入），日志前缀 `audit event dropped:` | 本次 |

> 全部经 `cargo fmt --check` + `cargo clippy --workspace -D warnings` + `tsc --noEmit` + 新单测，已 push 到 dev。下方原始清单保留作完整记录。

---


---

## A. 安全

✅ **基线很稳**（核实）：OIDC 默认 JWKS 验签（`routes/auth.rs`，免验签仅 dev 开关）；HTTP handler **逐个自守 RBAC**（27/30 调 `require_session` 带角色）；`/mcp` 既需 `mcp_enabled` 又需 `require_session(Operator)` 且默认关（`mcp/transport_http.rs:15-33`）；egress allowlist **请求时真强制并审计**（`routes/ask.rs:38`、`models.rs:22/104`、`mcp/tools_impl.rs:190`）；CORS origin 白名单；生产代码 0 unwrap。

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| S2(E6) 限流 + G2 request-id/trace | `middleware.rs`：按 IP 固定窗口限流（登录/admin 严格桶 20/min、其余 600/min，可 env 调），超限 429+Retry-After；request-id 中间件透传/生成 `x-request-id` 并建 tracing span（检索链路日志可按请求聚合），响应回写头部 | 本次 |
| G1(E5) OpenAPI | `routes/openapi.rs`：表驱动生成 OpenAPI 3.1，`GET /api/openapi.json` 暴露；路由表覆盖全 33 个 REST 方法，附漂移自检单测 | 本次 |
| P2/P1(E7) 50k 压测 | `examples/perf_scale.rs`：101k chunks 顺序 P50/P95=251/330ms，并发 8 P50=1574ms，**争用系数 6.26×**→证实单 Mutex 串行化读，P1 改造收益明确（见 `docs/qa/PERF_SCALE_50K.md`） | 本次 |
| D2(E4) 检索 CI 质量门 | `retrieval_regression --assert-thresholds` + 自带纯文本 fixture 套件（`docs/qa/ci_fixtures/` 12 文档 + `retrieval_regression_ci.json` 14 case）；新增 CI job `retrieval-gate`（offline 确定性，阈值见 `retrieval_regression_ci_thresholds.json`），排序/gating 退步即拦 | 本次 |

> 下表为审计原始清单（保留）。

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| S1 | 🟡 中 | 远程 API Key 以明文存 `settings.json`，无 OS keychain | `memori-desktop/src/model_settings.rs` `api_key`/`remote_api_key` | 新增（建议接 keyring） |
| S2 | 🟡 中 | **无任何接口限流**，登录/admin 可被暴力 | server 无 governor/tower::limit | E6 |
| S3 | 🟡 中 | 无登出/会话主动失效，仅 8h TTL | `main.rs:38 DEFAULT_SESSION_TTL_SECS`，无 logout 路由 | E2 |
| S4 | 🟢 低 | CORS `allow_methods/headers(Any)` 偏宽（origin 已白名单） | `routes/mod.rs:86-87` | E3 |
| S5 | 🟢 低 | 审计写失败是 warn-and-drop（静默丢审计），无完整性链 | `audit.rs:38` | 新增（IO 失败应升级告警） |
| S6 | 🟢 低 | 桌面 `open_source_location` 不校验路径是否在 watch_root/scope 内，可 reveal 任意存在文件 | `commands/scope.rs:55-90` | 新增（defense-in-depth；本地信任，低危） |

---

## B. 检索 / 作答质量（来自 v2 失败分析）

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| Q1 | 🔴 高 | A 类 gating 误拒：证据已召回却被阈值 55 砍（多格式抽取 1/6、长文 0/2） | baseline v2「失败分析 A」 | 质量轮 |
| Q2 | 🔴 高 | 长文深埋事实 0/2（`V101/V102`，召回到 doc rank1 但 chunk rank2-4 被拒） | baseline v2「长文」 | 质量轮（Parent-Doc 扩展） |
| Q3 | 🔴 高 | **评测盲区**：harness 从不给 LLM 答案文本判分，只用 top-k 当代理，"答得对不对"无客观分 | `retrieval_regression.rs` 无 answer-judge | 新增（接 LLM-judge） |
| Q4 | 🟡 中 | 跨语言 V119：中文问→英文邮件埋点例外完全漏召 | baseline v2「跨语言」 | 质量轮（双语 query 扩展） |
| Q5 | 🟡 中 | 诱饵代号拒答泄露（`V086/V087/V092`），需语义级核验，有误伤风险 | baseline v2「失败分析 B」 | 质量轮（谨慎） |
| Q6 | 🟡 中 | OCR 缺失：图片/扫描件 0/4 不可检索 | baseline v2「图片/扫描」 | 质量轮（大功能） |

---

## C. 扩展性 / 性能

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| P1 | 🔴 高 | **单 `std::sync::Mutex<Connection>` 串行化所有读+写**，guard 持有整查询周期；唯一访问口 `lock_conn()` | `memori-storage/src/store.rs:169`、`lib.rs:527` | E7 后改造（WAL 只读连接池+单写 或 r2d2），需压测证据先行 |
| P2 | 🟡 中 | 50k 规模从未压测，无 `doc/chunk/merge/answer_ms` P50/P95 | plan 成功标准未验 | E7 |
| P3 | 🟢 低 | 图谱构建长文仍 ~7 分钟/2.5 万字（已提速 4-5×） | baseline v2「图谱速度」 | 已大幅缓解，留档 |

---

## D. 工程 / 可维护性

✅ 上帝文件已拆（lib.rs 2388→776 等）；0 TODO/FIXME/HACK；CI 强制 `clippy -D warnings`+fmt+全量测试。

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| D1 | 🟡 中 | 重复 model-helper 漂移风险（`models-helpers.ts` 20 export ≈ `modelUtils.ts`） | `ui/.../tabs/` 两文件 | E1（速赢） |
| D2 | 🟡 中 | 检索质量无 CI 阈值门，指标静默退步无人拦 | `rust-ci.yml` 仅 fmt/clippy/test | E4 |

---

## E. 前端

✅ 0 TODO、仅 6 处 `any`、1 处 console.*、ModelsTab 远程配置已重构。

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| F1 | 🟡 中 | **无 React ErrorBoundary**，任一渲染异常白屏整个 App | `ui/src` 无 ErrorBoundary/componentDidCatch | 新增（包 App 根） |
| F2 | 🟢 低 | 6 处 `any`/`as any` 削弱类型安全 | `ui/src` | 留档（逐步收紧） |

---

## F. CI / 构建 / 依赖

| ID | 严重度 | 问题 | 证据 | 处置 |
|---|---|---|---|---|
| C1 | 🟡 中 | rust-ci **仅 `ubuntu-latest`**，无 Windows/macOS PR 检查矩阵——而主用户在 Windows、且有已知 GBK 问题；跨平台问题只能等 release 才暴露 | `rust-ci.yml:18 runs-on: ubuntu-latest` | 新增（加 windows/macos 矩阵） |
| C2 | 🟡 中 | 无依赖漏洞扫描（无 cargo-deny/cargo-audit/npm audit） | CI 无相关 step | 新增 |
| C3 | 🟢 低 | Windows GBK→UTF-8 错误串乱码（旧 P2-9，需在 Windows 上复核是否仍现） | 旧 baseline 记录 | 与 C1 一起核 |

---

## G. 产品化 / 可运维

| ID | 严重度 | 问题 | 处置 |
|---|---|---|---|
| G1 | 🟡 中 | memori-server 无 OpenAPI/Swagger，第三方/UI 靠读代码对接 | E5 |
| G2 | 🟢 低 | 检索链路无 request-id/trace 可观测性 | E6 |
| G3 | 🟢 低 | 无多租户隔离：OIDC 登录后共享同一库 | E9（需求确认后） |
| G4 | 🟢 低 | 索引长任务无实时进度通道（SSE/WS） | E10 |

---

## H. 文档

✅ 本轮 `plan.md` / `IMPROVEMENTS.md` 已对齐现实。

| ID | 严重度 | 问题 | 处置 |
|---|---|---|---|
| H1 | 🟡 中 | README 可能让外部高估成熟度，未用状态徽标区分"已验证 vs 设计中" | E8 |

---

## I. 仓库卫生

| ID | 严重度 | 问题 | 处置 |
|---|---|---|---|
| I1 | 🟢 低 | `docs/superpowers/plans/2026-06-05-email-memory-source.md` 长期 untracked | 决定 track 或加 `.gitignore`（本轮不动） |

---

## 汇总：按严重度

- **🔴 高（5）**：P1 单连接串行化、Q1 gating 误拒、Q2 长文、Q3 作答层评测盲区。（其中 Q1/Q2/Q3 属下一个"质量轮"，P1 属本阶段 E7→改造。）
- **🟡 中（12）**：S1 密钥明文、S2 限流、S3 会话失效、Q4 跨语言、Q5 诱饵代号、Q6 OCR、D1 重复模块、D2 CI 质量门、F1 ErrorBoundary、C1 跨平台 CI、C2 依赖扫描、G1 OpenAPI、H1 README。
- **🟢 低（9）**：S4 CORS、S5 审计丢失、S6 路径校验、P3 图谱、F2 any、G2 trace、G3 多租户、G4 进度推送、I1 untracked。

## 与当前阶段（Phase 9 工程硬化）的映射

本阶段已覆盖：E1(D1)、E2(S3)、E3(S4)、E4(D2)、E5(G1)、E6(S2/G2)、E7(P1/P2)、E8(H1)、E9(G3)、E10(G4)。

**审计新暴露、原 backlog 未含、建议补进 Phase 9 的项**：
- **S1** 密钥明文 → 接 OS keychain
- **S5** 审计写失败静默 → IO 失败升级告警
- **S6** 桌面 open 路径 scope 校验
- **F1** React ErrorBoundary（成本极低、防白屏，建议优先）
- **C1** CI 跨平台矩阵（Windows/macOS）
- **C2** 依赖漏洞扫描（cargo-deny + npm audit）
- **Q3** 作答层 LLM-judge（评测可信度的根本闭环，跨"质量轮"但价值高）
