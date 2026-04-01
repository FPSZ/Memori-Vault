# Memori-Vault Plan Change Log

来源：`docs/plan.md`

## Change Log
- 2026-03-13: 扩展 Phase 7.1 中文检索稳定性修复条目，写入 GPT 完整修复计划（Key Change 1-5）及审查发现的 5 个实现缺口（docs_phrase gating 穿透、降权路径未定义、虚词过滤层次错误、term_coverage 分母未定义、dominant-term penalty 前置依赖）
- 2026-03-13: 新增 Phase 7，整合竞品技术对比分析结论（TimeDecay、Parent Document Expansion、Primacy-Recency）、迁移测试缺口、图谱差异化方向与工程待办；删除根目录临时 PLAN.md
- 2026-03-11: 初始化检索大重构计划书，锁定 AST + SQLite + Hybrid + Graph Secondary 路线
- 2026-03-11: 调整计划目标，明确 50,000 份文档规模下优先解决文档级精确定位，再做片段级检索与证据链
- 2026-03-11: 新增 `docs/RETRIEVAL_BASELINE.md`，固化当前检索链路、运行边界与失败类型
- 2026-03-11: 建立机器可执行回归查询集，当前以 `docs/retrieval_regression_suite.json` 作为唯一执行源
- 2026-03-11: `memori-parser` 升级为基于 `pulldown-cmark` 的 AST 语义分块，并补充单测
- 2026-03-11: 吸收计划评审意见，将向量存储规模、parser 重构后的索引失效策略、document routing 表示生成策略、SQLite 连接模型与持续回归节奏并入正式计划
- 2026-03-11: 落地 parser/index 版本元数据、`system_metadata`、强一致全量重建、搜索阻断与重建状态透传，正式关闭 Phase 1 的旧索引失效处理策略任务
- 2026-03-11: Phase 2 完成第一轮存储底座收敛：新增 `file_catalog`、`documents_fts`、`chunks_fts`，写路径切换为结构化 `replace_document_index`，并通过活动 catalog 过滤与 `INDEX_FORMAT_VERSION = 2` 保证旧库自动重建
- 2026-03-11: 新增 `memori-core/examples/phase2_diagnose.rs`，完成 Phase 2 的 dense 存储与 SQLite 连接模型决策闭环，正式进入 Phase 3
- 2026-03-11: 完成 Phase 3 第一轮主链路：`document routing -> candidate-doc chunk retrieval -> chunk RRF -> strong evidence gating -> structured ask response`，并让 desktop/server/UI 切换到结构化 citations 与 evidence 展示，旧字符串入口继续兼容
- 2026-03-11: 完成 Phase 3 第二轮精度增强：新增 query analysis、CJK / mixed-token term 展开、deterministic 文档信号检索、文档级融合，以及 `query_analysis_ms / doc_lexical_ms / doc_merge_ms` 调试指标
- 2026-03-11: 新增 `docs/retrieval_regression_suite.json`、`retrieve_structured(...)`、`RuntimeRetrievalBaseline` 与 `memori-core/examples/retrieval_regression.rs`，为 Phase 0 / Phase 6 的可重复回归执行与基线采集提供统一入口
- 2026-03-11: 回归 runner 升级为双轨模式：新增 `offline_deterministic` / `live_embedding`、`profile_tags`、离线确定性索引构建、live health check 与分层 baseline 表达，继续为 Phase 0 / Phase 6 的可执行验收收口
- 2026-03-11: 新增共享 `Model Egress Policy` 内核，并把 `local_only / allowlist` 企业出站策略落到 desktop、server 与设置 UI，默认本地优先且不允许通过环境变量或远端 provider 绕过策略
- 2026-03-11: 跑通 offline regression 的 `core_docs` 与 `repo_mixed`，并记录 live regression 在本地 Ollama 不可达时的结构化阻塞结果，正式补齐 Phase 0 的 runtime baseline 与 retrieval metrics 文档
- 2026-03-11: 同步更新 `docs/RETRIEVAL_BASELINE.md`、`docs/enterprise*.md` 与 `docs/RELEASE_CHECKLIST.md`，把企业本地优先运行态验收纳入正式交付闭环
- 2026-03-11: 收口文档边界：`docs/plan.md` 与 `docs/RETRIEVAL_BASELINE.md` 作为正式项目文档进入仓库，`docs/retrieval_regression_suite.json` 作为唯一回归执行源，删除重复的 Markdown suite 镜像，避免同一事实维护两份
- 2026-03-11: 完成“检索质量硬化第一轮”的 document routing / gating 收口：新增 strict FTS、deterministic document search signal、dense 结果的直接 lexical 支撑、missing-file lookup 拒答校验；离线回归把 `reject correctness` 提升到 `core_docs=0.9592`、`repo_mixed=0.9608`，但文档级命中仍需继续提升
- 2026-03-11: 先完成 regression suite drift reconciliation，再推进 document routing 第二轮：移除/修正漂移样本、把实现真值迁移到代码/UI 目标文档，并重建干净离线基线
- 2026-03-11: 完成 document routing 第二轮第一步提准：`document_search_text` 改为跨文档抽样 snippet，提升高辨识度 document signal 权重；离线回归提升到 `core_docs: Top-1=0.7273 / Top-3=0.7576 / Top-5=0.8485 / Reject=1.0000`，`repo_mixed: Top-1=0.5682 / Top-3=0.5909 / Top-5=0.6364 / Reject=0.9800`
- 2026-03-11: 补入当前真实基线口径：最新离线回归更新为 `core_docs: Top-1=0.6970 / Top-3=0.6970 / Top-5=0.7576 / Reject=1.0000`，`repo_mixed: Top-1=0.4773 / Top-3=0.4773 / Top-5=0.5455 / Reject=0.9400`；明确这些结果仅来自 6/11 文档的小样本语料，不能表述成 50,000 文档规模精度结论
- 2026-03-11: 更新文档口径为“企业本地优先运行时已收口，但 mixed corpus 检索质量仍未达交付线”，避免把本地优先策略成熟度误写成整体检索质量成熟度
- 2026-03-12: 桌面端模型未配置流程收口为“无 runtime / 无 onboarding / 搜索框内联红字提示”，明确当前 active provider 未完成配置时不再自动回退本地 Ollama
- 2026-03-12: answer panel UI 收口到当前基线：回答区使用独立图标，`Citations` 默认折叠，`Evidence` 按文档聚合去重并以两栏卡片展示，`Retrieval Metrics` 改为横向阶段排行并显式区分总耗时与未打点部分
- 2026-03-12: 新增更具体的 docs-query 诊断：`岗位是什么` 与 `新增的12岗位是什么` 可答，但 `新增的岗位是什么` 仍会被高频业务词 `新增` 带偏；确认下一轮优先修“多词覆盖优先于 broad lexical 泛命中”，而不是继续堆单词命中权重
- 2026-03-12: 建立 `docs/STRUCTURE.md` 作为内部结构地图，并固定大文件拆分路线图：优先 `ui/src/App.tsx`、`memori-desktop/src/lib.rs`、`memori-server/src/main.rs`；`memori-core/src/retrieval.rs` 与 `memori-storage/src/document.rs` 暂缓拆分
- 2026-03-11: 收口本地测试入口：取消 `scripts/` 整目录忽略，新增 `scripts/test-retrieval.ps1` 作为回归 runner 包装脚本，并把 `smoke-start.ps1` / `smoke-stop.ps1` 升级为支持 `desktop/server/both` 与 `-SkipModelCheck` 的当前 smoke 入口
- 2026-03-11: 吸收 release-note 末尾评审中的有效部分：恢复 docs query 的 deterministic document signal 输入，避免 `document_signal_query(...)` 在描述型问题上退成空字符串；document-dense 与 FTS tokenizer 重配保留为后续精度议题，不在本轮 recovery pass 直接硬上
- 2026-03-11: 使用新脚本重跑离线基线后，当前最新快照更新为 `core_docs: Top-1=0.6667 / Top-3=0.6667 / Top-5=0.6970 / Reject=1.0000`，`repo_mixed: Top-1=0.5000 / Top-3=0.5227 / Top-5=0.5682 / Reject=0.9600`；说明这轮修正有效但仍未恢复到 `repo_mixed Top-1=0.5682` 旧高点
- 2026-03-12: 明确 mixed-script 实体检索修复口径：禁止为具体实体名或具体问法语义开后门，统一改为通用 CJK query backoff 与中英脚本边界切分规则
- 2026-03-11: 接受“当前离线回归数字不足以代表产品可用性”的判断，新增外部本地 10 文档 / 15 问可用性 smoke gate 作为第一放行标准；在 gate 通过前，`core_docs / repo_mixed` 仅继续作为内部回归参考

