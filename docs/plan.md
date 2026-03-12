# Memori-Vault Retrieval Rebuild Plan

Last Updated: 2026-03-12 UTC  
Current Phase: Phase 6 - Validation  
Overall Progress: 88%

## Status Rules
- 任务状态：完成即勾选 `- [x]`，不保留“半完成”状态。
- 阻塞标记：若有阻塞，在任务后追加简短括注，如 `(blocked: sqlite-vec build)`.
- 状态同步：每次勾选时，同步更新顶部的 `Last Updated`、`Current Phase`、`Overall Progress`。
- 日志追加：关键决策、阶段切换、验收结果记录在文末 `Change Log`。
- 回归节奏：每完成一个阶段的可验收工作，至少回跑一次当前回归集，不把全部验证堆到 `Phase 6`。

## 重构目标
重建 Memori-Vault 的检索主链路，使系统在本地优先、纯 SQLite 底座下，同时实现：
- 目标态是在 50,000 份文档规模下，稳定而精确地定位目标文档与目标片段
- 更高的检索精度与更稳定的命中排序
- 更强的证据链与拒答机制
- 更清晰的 scope / watch_root / source traceability
- 更低的检索延迟与更可控的索引复杂度

## 已锁定架构决策
- `Parser`：基于 `pulldown-cmark` 的 AST 语义分块
- `Storage`：纯 SQLite 底座
- `Retrieval`：两级轻量增强架构
  - 第一级：文档级召回（document routing）
  - 第二级：片段级召回（chunk retrieval）
  - 融合方式：单向量 + FTS5 + RRF
- `Graph`：仅作后置上下文增强，不参与主召回
- `Answer Mode`：默认强证据模式；支持双模切换

## 成功标准
- 检索质量：
  - 已知回归集 `Top-1 document hit >= 90%`
  - 已知回归集 `Top-3 document recall >= 98%`
  - 已知回归集 `Top-5 chunk recall >= 95%`
  - 文件/片段溯源有效率 `= 100%`
- 回答可信度：
  - 有证据问题必须返回真实引用
  - 无证据问题必须拒答或切到显式“开放模式”
- 性能：
  - 50,000 份文档规模下，冷启动后检索保持本地可交互
  - 记录并评估 `doc_recall_ms`、`chunk_recall_ms`、`merge_ms`、`answer_ms` 的 `P50 / P95`
- 架构：
  - `memori-parser`、`memori-storage`、`memori-core`、`memori-desktop` / `memori-server`、`ui` 边界清晰，无新增循环依赖

## 当前真实状态
- 当前最新离线回归只覆盖仓库内的两个小语料：
  - `core_docs`：`6` 份文档，`267` 个 chunk
  - `repo_mixed`：`11` 份文档，`759` 个 chunk
- 当前最新离线指标：
  - `core_docs`：`Top-1=0.6667`、`Top-3=0.6667`、`Top-5=0.6970`、`citation validity=1.0`、`reject correctness=1.0`
  - `repo_mixed`：`Top-1=0.5000`、`Top-3=0.5227`、`Top-5=0.5682`、`citation validity=1.0`、`reject correctness=0.96`
- 这不是 50,000 文档规模的精度验证结果，只是当前 checked-in 小样本回归基线。
- 当前可以确认的是“引用可信度强于文档排序质量”：
  - 有答案时，citation 仍然可信
  - mixed corpus 下，文档级 Top-1 还不足以支撑高精度对外交付口径
- 当前最大的阻塞已经不是“某个离线指标还差几个点”，而是**产品可用性仍未过线**：
  - shipping ask path 仍频繁出现 `insufficient_evidence`
  - 或者状态显示为 `answered`，但正文实际输出“当前上下文不足”
  - 因此现有 `core_docs / repo_mixed` 数字只能继续作为内部回归参考，不能当作“已经可交付”的主要依据
- 桌面端启动体验已经按“未配置则不启 runtime”收口：
  - 首次或当前 active provider 未完成配置时，不再自动回退本地 Ollama
  - 不再用大 onboarding 弹窗打断启动
  - 搜索框会以内联红字提示“未配置模型，请在 设置 > 模型 中配置”，并保持禁用
- 当前 answer panel 的信任呈现已经收口到较稳定的 UI 基线：
  - `Citations` 默认折叠
  - `Evidence` 按文档聚合并去重，再以两栏卡片展示
  - `Retrieval Metrics` 改为横向阶段排行，并单独展示 `总耗时 / 已打点小计 / 未打点部分`
- 结构治理已进入“文档先行”阶段：
  - 新增 `docs/STRUCTURE.md` 作为内部结构地图
  - 后续拆分优先级固定为 `ui/src/App.tsx -> memori-desktop/src/lib.rs -> memori-server/src/main.rs`
  - 当前不把结构拆分与 retrieval 提准混做同一轮
- 当前又新增确认了一类更具体的 docs-query 排序缺陷：
  - 像 `岗位是什么` 这类核心词清晰的问法可以回答
  - 像 `新增的12岗位是什么` 这种带强约束数字的问法也可以回答
  - 但 `新增的岗位是什么` 这种“高频业务词 + 核心名词”的组合仍可能被 `新增客户 / 新增合作 / 新增指标` 等无关文档带偏
  - 这说明当前真实问题不是“岗位类问题完全搜不到”，而是**中文描述型 query 的 broad lexical 污染过强，而多词覆盖奖励不足**
- Phase 6 从现在起新增一个更高优先级 gate：
  - 使用**外部本地小语料**做 10 文档 / 15 问可用性 smoke
  - 只有当 `usable_answer_count >= 10 / 15`，且不再出现“`answered` + `当前上下文不足`”假通过时，才允许继续把 retrieval 提准视为主目标
- `live_embedding` 仍被本地 Ollama / embedding 可用性阻塞，Phase 6 还不能关闭。

## Phase 0: Baseline & Diagnosis
**Goal**  
固化现状问题、基线数据和验收样本，避免重构后“感觉变好了”但无法证明。

**Tasks**
- [x] 盘点当前检索链路，写清 `query -> scope -> retrieval -> answer -> citations` 数据流
- [x] 固化 30-50 条高价值回归查询集，覆盖中文、英文、混合中英、代码标识符、人名/代号、文件名指向
- [x] 为每条回归查询标注目标文档、目标片段、允许的候选范围与拒答预期
- [x] 记录当前 `watch_root`、`scope`、`db_path`、`embedding model key`、`embedding dim`
- [x] 固化当前检索指标：`Top-1 document hit`、`Top-3 document recall`、`Top-5 chunk recall`、来源有效率、拒答行为、主要耗时
- [x] 收集当前已知失败案例：错引、旧路径、低相关度、找不到文件、应拒答未拒答

**Exit Criteria**
- 有一份可重复执行的回归样本与基线记录
- 能明确说出当前系统最常见的 3-5 类失败模式

**Notes / Risks**
- 样本必须包含“中文问法 + 英文标识符”混合查询
- 样本必须包含“文件存在但当前 scope 不包含”的场景

## Phase 1: Parser Rebuild
**Goal**  
把旧的文本切分升级为结构感知的 Markdown 语义分块器。

**Tasks**
- [x] 引入并固定 `pulldown-cmark` 解析路径
- [x] 按标题边界切块，并提取标题层级元数据
- [x] 保证 `code block` 与 `list` 不被中途切断
- [x] 对超长无结构段落实现安全回退切分，并保留 overlap
- [x] 为 chunk 增加结构元信息：标题路径、块类型、顺序位置
- [x] 保持 `parse_and_chunk` 上层接口稳定或提供明确迁移层
- [x] 明确 parser 重构后的旧索引失效处理策略并写入文档

**Exit Criteria**
- 复杂 Markdown 输入的 chunk 边界符合阅读直觉
- 代码块、列表、表格附近不出现明显语义撕裂
- 明确 parser 重构后的旧索引失效策略，避免新旧 chunk 边界混用

**Notes / Risks**
- AST 无法完美覆盖所有 Markdown 方言，未知结构必须可降级
- 分块边界变化默认视为索引失效事件；若不兼容旧索引，必须显式触发全量重建或等价迁移

## Phase 2: SQLite Retrieval Base
**Goal**  
收敛并重构 SQLite 检索底座，使文档级索引、片段级索引、FTS、catalog、scope 和路径约束一致。

**Tasks**
- [x] 收敛 `documents` / `chunks` / `file_catalog` / `file_index_state` 的职责边界
- [x] 在存储层明确区分文档级索引与片段级索引，避免所有检索都直接扫 chunk
- [x] 重审 `chunks_fts` 结构，明确同步策略（trigger 或显式写入）并统一实现
- [x] 增加文档级 FTS 或文档级可检索摘要字段，为 document routing 做准备
- [x] 明确 dense 向量存储方案：验证 `sqlite-vec` 可行性；若暂不采用，写清内存上限、适用规模与回退策略
- [x] 统一 `file_path`、`watch_root`、`scope` 的规范化规则
- [x] 明确旧 DB 迁移策略、失效索引清理策略与 parser 重构后的全量重建流程
- [x] 评估 SQLite 连接模型，明确单连接 `Mutex` 是否继续接受，还是切到更适合混合检索的读写策略
- [x] 为“来源文件不存在 / 不在当前资料库”建立检测与标记机制

**Exit Criteria**
- SQLite 层能稳定表达文档、chunk、词法索引、路径范围与索引状态
- 不再出现“当前库里没有这个文件，但还能被命中”的默认行为
- 在目标规模下，向量存储与 SQLite 连接模型都有明确边界，而不是把风险推迟到 `Phase 3`

**Notes / Risks**
- Windows / macOS / Linux 路径规范化要统一
- 旧 DB 迁移不能默默带入错误 scope
- `50k+` 文档规模下，单 `Mutex<Connection>` 可能成为混合检索瓶颈，本阶段必须给出是否接受的结论

## Phase 3: Hybrid Retrieval
**Goal**  
建立稳定可解释的两级 `dense + lexical + RRF` 主召回链路。

**Tasks**
- [x] 明确 document routing 主路径：先召回候选文档，再在候选文档内做 chunk 检索
- [x] 明确 document routing 的文档表示生成策略，避免依赖额外 LLM 摘要步骤
- [x] 明确 dense 检索主路径与向量存储接口
- [x] 明确 lexical 检索主路径与 FTS 查询构造规则
- [x] 处理 CJK 与 mixed-token 查询，不丢英文标识符/代码 token
- [x] 实现文档级 RRF 融合与片段级 RRF 融合，保留 `reason` 与原始分数
- [x] 实现结果去重与层级排序：先文档排序，再片段排序
- [x] 去掉不可解释的魔法分数映射，统一语义分数定义
- [x] 增加 answerability gating：无可信证据时直接拒答或切开放模式

**Exit Criteria**
- 已知代号、术语、轻微改写问题都能稳定命中目标文档
- 返回结果可解释，且不会因错误阈值裁掉正确结果

**Notes / Risks**
- 不引入 cross-encoder 或多向量模型
- 若接入 `sqlite-vec`，其构建与跨平台分发要单列验证
- 文档级向量或词法表示必须来自可重复的本地确定性规则，不能依赖昂贵或难维护的摘要链路

## Phase 4: Graph As Secondary Layer
**Goal**  
将图谱从主召回链路中剥离，降为检索命中后的增强上下文层。

**Tasks**
- [ ] 识别并移除图谱参与主召回的路径
- [ ] 仅基于 Top-K chunk/document 获取图关系补充
- [ ] 统一图谱补充格式，限制查询复杂度与 fan-out
- [ ] 保证图谱失败不阻塞主答案生成

**Exit Criteria**
- 图谱不再决定主命中结果
- 图谱缺失或失败时，核心检索与回答仍可工作

**Notes / Risks**
- 严禁回到“图谱先行”的重路径

## Phase 5: UX & Trust
**Goal**  
把底层证据链与范围控制清晰传递给用户。

**Tasks**
- [x] 展示规范化 citations UI，而不是只展示松散来源块
- [x] 增加 evidence line，显示文档、片段、命中原因、原始分数与最终排序位置
- [x] 点击引用可定位本地文件与片段
- [ ] 默认文件快速定位只在当前资料库内工作
- [ ] UI 明确区分：当前 scope、当前资料库、开放回答模式
- [x] 暴露调试信息：`doc_recall_ms`、`chunk_recall_ms`、`merge_ms`、`answer_ms`

**Exit Criteria**
- 用户能快速判断“答案是不是来自我的资料”
- 用户能清楚知道“当前搜的是哪个范围”

**Notes / Risks**
- 协议要支持结构化来源返回，而不是长期依赖字符串切分

## Phase 6: Validation
**Goal**  
完成全链路验收，并先把产品从“有回归数字”拉回到“真实可用”。

**Tasks**
- [ ] 固定外部本地 10 文档 / 15 问可用性 smoke gate，作为当前最高优先级验收入口
- [ ] 用 shipping ask path 跑完 15 题，并记录每题 `status / answer / citations / evidence / failure_class`
- [x] 封死 `answered` 假通过：当最终生成文本包含“当前上下文不足”或等价拒答语义时，不再对外标记为 `answered`
- [x] 桌面端未配置模型时改为“无 runtime / 无 onboarding / 搜索框内联红字提示”
- [x] 收口 answer panel：引用默认折叠、证据按文档聚合去重、检索指标按阶段排行展示
- [x] 建立内部结构地图文档（`docs/STRUCTURE.md`）并固定下一轮拆分优先级
- [ ] 修复中文描述型 docs query 的 broad lexical 污染：让多词覆盖优先于单个高频业务词命中
- [ ] 跑完 Phase 0 定义的全部回归查询集 (blocked: live_embedding full_live blocked by local Ollama availability)
- [x] 比较重构前后的 `Top-1 document hit`、`Top-3 document recall`、`Top-5 chunk recall`、citation validity、拒答正确率
- [ ] 对大规模文档集做本地性能压测并记录 `P50/P95`
- [ ] 验证无证据问题会正确拒答或切开放模式
- [ ] 进行 Windows / macOS / Linux 构建检查

**Exit Criteria**
- 先通过小样本真实语料 gate：`usable_answer_count >= 10 / 15`
- 不再出现“`answered` + `当前上下文不足`”
- 通过题必须同时满足“答案对 + 引用对 + 不是模板废话”
- 在此基础上，再继续看精度、可信度、性能三项是否达到成功标准
- 无新增 panic、无明显路径污染、无默认跨库误命中

**Notes / Risks**
- 必须单独复测“历史 DB 迁移后”的行为
- 当前 Phase 6 的第一 blocker 已明确切换为**产品可用性 gate 未通过**，不是单一离线指标
- 可用性 smoke 的失败必须按三类显式记录，不再统一混成“准确率低”：
  - `retrieval_miss`
  - `gating_false_refusal`
  - `answer_synthesis_fail`
- 已完成 suite drift reconciliation：当前 JSON suite 的 `answer` case 已通过“target document exists + target clue exists”机械核验
- 当前 Phase 6 的核心阻塞已经从“样本漂移”转成“真实精度不足”：
  - `core_docs` 虽然可作为 docs-only 基线，但 `Top-1/Top-3` 仍未达到目标
  - `repo_mixed` 当前最新结果恢复到 `Top-1=0.5000`，但仍低于此前 `0.5682` 快照，也明显低于交付线
- 当前真实 gap 主要集中在两类：
  - 描述型文档查询的 document routing 仍然不稳，例如 `R02`, `R05`, `R13`, `R21`, `R28`, `R35`, `R36`
  - mixed-token / implementation lookup 仍然大量排不到正确代码文件，例如 `R40`, `R42`, `R43`, `R44`, `R45`, `R46`, `R50`, `R51`
- 当前对 docs-query 误排又多了一条更明确的诊断：
  - 不是“关键词完全没识别到”
  - 而是像 `新增` 这类高频业务词会把很多无关文档一起拉进来
  - 当前 document/chunk 排序对“同时命中多个有效约束词”的奖励还不够强
  - 下一轮应优先做：
    - 中文 query skeleton 词降权而不是硬删除
    - document routing 的 informative-term coverage 排序
    - chunk rerank 的多词覆盖优先
    - gating 的 top-N 证据集中度判断
- 这轮 retrieval 修复口径继续固定为 precision-first：
  - 不为了“更常回答”放松企业场景正确性
  - 不对任何具体实体名、客户语料、问法写硬编码后门
  - 变更范围优先限制在 `query normalization / document ranking / chunk ranking / gating`，方便必要时快速回滚
- 当前新增确认的一类泛化缺陷是 **mixed-script 实体检索泛化不足**：
  - 中英实体贴连或混写时，英文问法能命中，中文问法可能掉成 `insufficient_evidence`
  - 当前修复方向固定为通用 CJK query backoff / script boundary tokenization，明确禁止对具体实体名或具体问法语义写 hardcode
- 2026-03-11 对 release-note 末尾的外部评审意见完成一次辩证收口：
  - 已接受并落地：`document_signal_query(...)` 不应让 docs query 退成空输入；测试脚本需要升级为当前 runner / smoke 入口
  - 暂不直接照搬：立刻引入 document-level dense；当前 recovery pass 仍先优先收词法/规则侧回退
  - 继续观察后再决定：FTS5 tokenizer 重配；问题存在，但要和现有 CJK query expansion、索引重建成本一起评估
- 当前计划文档里的 `50,000` 规模目标仍然是目标态，不应被解读为已经验证完成

## Change Log
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
