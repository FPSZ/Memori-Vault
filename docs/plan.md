# Memori-Vault Retrieval Rebuild Plan

Last Updated: 2026-03-13 UTC
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

## Phase 7: Retrieval Enhancement — Next Round

**Goal**
在 Phase 6 可用性 gate 通过后，进一步提升检索质量与产品差异化。

### 7.1 检索质量提升

**TimeDecay 时间衰减（P1，改动小）**
- 参考：指数衰减模型 `0.5^(age/halfLife)`，半衰期 30 天
- 位置：`memori-core/src/retrieval.rs` 的 `final_score` 计算
- 思路：在 chunk 合并排序时，对 `mtime_secs` 做指数衰减加权，最近修改的文档排名更靠前
- 前提：`file_catalog.mtime_secs` 已有，直接可用
- [ ] 实现并在回归集上验证不降低 Top-1

**Parent Document Expansion（P1，效果明显）**
- 参考：LlamaIndex Small-to-Big Retrieval 策略
- 位置：`memori-core/src/retrieval.rs` 证据链构建阶段
- 思路：当某个 chunk 的 `final_score >= 0.85` 时，自动拉取同文档所有 chunk 合并上下文，上限 8000 字符
- 效果：避免长文档只拿到碎片，回答更完整
- [ ] 实现扩展逻辑，加字符上限防止 context 爆炸
- [ ] 验证扩展后 answer 质量提升，延迟可接受

**Primacy-Recency U 型排序（P2）**
- 参考：arXiv 2307.03172 (Stanford/UCB "Lost in the Middle")
- 位置：LLM context 构建时的 chunk 排列顺序
- 思路：最高分 chunk 放首位，次高分放末位，中间分数的放中间，对抗 LLM 注意力在长 context 中间衰减
- [ ] 实现 chunk 重排序，在回归集上验证

**中文描述型 query 检索稳定性修复（P1，已在 Phase 6 识别）**

根因分析（三层叠加，缺一不可）：
1. `broad lexical` 对高频中文词过宽，单词命中即算"有证据"（`retrieval.rs:601`）
2. 元分析文档（如 `docs/AI.md`）含测试问题原文，通过 `docs_phrase` 抢排名（`retrieval.rs:305-307`，`docs_phrase` 优先级 = 7）
3. gating 侧 `has_strong_document_signal` 把 `docs_phrase` 也算强信号，排序污染直接穿透到放行（`retrieval.rs:618`）

GPT 修复计划（泛化去噪 + 覆盖率门控，无实体硬编码）：

**Key Change 1 — Query 去噪**
- 剥离中文问句尾巴后的语义核心词（`CJK_QUESTION_SUFFIXES` 已有，需扩展）
- 新增通用中文虚词过滤（"的/了/吗/呢"等）降低高频噪声词权重
- 限制 CJK 前缀回退词数量与最短长度
- term 分级输出：`specific`（数字/标识符/长词）与 `generic`（短高频词），供排序与门控复用

**Key Change 2 — 排序信号升级**
- 新增 `distinct_term_hits` 与 `term_coverage`（命中不同词数 / 查询有效词数）
- 对"单一 generic 词反复命中多个 chunk"的文档施加 dominant-term penalty
- `docs_phrase` 仅在短语包含至少一个 `specific` term 时给高权重；纯问句模板短语不再强提升

**Key Change 3 — 拒答门控重构**
- 替换 `top_doc_any_lexical >= 2` 为覆盖率条件：
  - 放行需满足：`distinct_term_hits >= 2` 且 `term_coverage >= 0.5`
  - 或：存在 `specific` 严格命中 + 同文档多片段一致支持
- 短查询（高歧义）继续保守拒答

**Key Change 4 — 可观测性**
- `RetrievalMetrics` 新增（向后兼容）：`top_doc_term_coverage`、`top_doc_distinct_term_hits`、`top_doc_dominant_term_ratio`、`gating_decision_reason`
- 失败归因记录：`retrieval_miss / gating_false_refusal / synthesis_fail`

**Key Change 5 — 测试语料治理**
- smoke 验收将元分析/交接文档标记为低优先级（通过问题集配置，不写运行时硬编码路径）

审查发现的实现缺口（需在动工前确认）：

> **缺口 1（高）：`docs_phrase` 污染在 gating 侧未堵住**
> Key Change 2 修了排序，但 `has_strong_document_signal` 仍把 `docs_phrase` 算强信号（`retrieval.rs:618`）。
> 若 docs/AI.md 仍排第一，gating 会因 `has_strong_document_signal=true` 直接放行。
> 修复必须配套：Key Change 2 降排序 + Key Change 3 同步降 `has_strong_document_signal` 中 `docs_phrase` 的权重。

> **缺口 2（中）：`docs_phrase` 降权的实现路径未定义**
> `document_reason_priority` 是纯函数，签名只有 `(reason: &str, query_family)`，拿不到 term 质量信息。
> 需明确选择：(a) 在 `document_reason` 字符串编码质量（如 `"docs_phrase_specific"` vs `"docs_phrase_generic"`），或 (b) 改函数签名传入额外上下文。

> **缺口 3（中）：虚词过滤的实现层次说错**
> "的/了/吗/呢"在 CJK 语境下不是独立 token，`extract_query_tokens` 按非 CJK 字符切分，整句是一个 token。
> 过滤必须在 `extract_cjk_query_phrases` 内部做字符级剥离，不是 token 级过滤。

> **缺口 4（中）：`term_coverage` 分母定义不明确**
> "查询有效词数"需明确：是剥离问句尾巴后的词数，还是所有 `chunk_terms` 数量？
> 对"新增的岗位是什么"，剥离后得 `["新增", "岗位"]`，分母 = 2；若含"是什么"则分母 = 3。
> 定义直接影响 0.5 阈值的实际效果。

> **缺口 5（低）：`dominant-term penalty` 缺前置条件**
> 当前 `direct_chunk_lexical_signal` 只返回 `(is_strict, score)`，不记录 per-term attribution。
> 实现 dominant-term penalty 需先在 chunk 级别追踪每个 term 的命中情况，应标注为依赖项。

- [ ] 确认 5 个实现缺口后开始动工
- [ ] 实现并在 `repo_mixed` 回归集上验证 Top-1 提升，`gating_false_refusal` 不上升

### 7.2 稳定性 / 迁移待验证

- [ ] trigram FTS 重建后，`replace_document_index()` 写入路径正常（GPT 审核指出的测试缺口）
- [ ] `ext` 旧表且同时缺 `parent_dir / removed_at` 的组合形状测试
- [ ] `documents_fts` 是 trigram+doc_id 但缺 `file_name` 的旧库能否被正确检测并重建
- [ ] FTS 重建后 `rebuild_state=required` 正确触发全量重建，词法检索恢复正常

### 7.3 差异化方向（不跟随竞品）

知识图谱是当前最大差异化点，竞品均无：
- [ ] 图谱可视化 UI（当前只有数据，没有展示入口）
- [ ] 图谱辅助检索（当前图谱数据存在但检索时利用不足）
- [ ] 跨文档关联推理（"A 文档提到的概念在 B 文档里有更详细的解释"）

### 7.4 工程 / 架构

- [ ] `memori-server` API 文档（当前无 OpenAPI spec）
- [ ] 多用户隔离（当前 OIDC 登录后共享同一个库）
- [ ] 增量索引进度推送（WebSocket 或 SSE）
- [ ] `memori-storage` 模块拆分合并（GPT 进行中，编译通过，待最终审核）

**Exit Criteria**
- TimeDecay 和 Parent Document Expansion 上线后，`repo_mixed Top-1 >= 0.60`
- 图谱可视化有基础 UI 入口
- 迁移测试覆盖上述 4 个缺口场景

---

## Phase 8: Full-Stack Architecture Hardening (Architect Ideas)

**Goal**
在不破坏“本地优先 + 纯 SQLite”主路线的前提下，补齐企业可运维、可扩展、可治理的后端能力。

### 8.1 Shell 收敛（desktop/server 复用）
- [ ] 提取 `shell-service` 共享层，收敛 `settings/model-policy/auth/audit` 的重复流程
- [ ] 统一 DTO 与错误码映射，避免 desktop/server 响应语义漂移
- [ ] 建立能力矩阵（desktop-only / server-only / shared），后续新增接口必须先归类

### 8.2 安全基线加固（Server First）
- [ ] OIDC 登录链路补 JWT 签名校验（JWKS/issuer/audience）
- [ ] 将 `CorsLayer::allow_origin(Any)` 收敛为 allowlist（dev/prod 分层配置）
- [ ] 会话管理增加“显式失效/登出/最长会话上限”策略，并补最小防重放约束
- [ ] 管理接口增加最小速率限制与审计字段标准化

### 8.3 存储与检索扩展性（50k+ 预备）
- [ ] 评估并落地 SQLite 连接模型升级（读写并发策略或连接池）
- [ ] 对比 `in-memory cache` 与 `sqlite-vec/ANN` 的检索延迟和资源占用，形成决策文档
- [ ] 把检索关键耗时纳入稳定基准：`doc_recall_ms/chunk_dense_ms/merge_ms/answer_ms` 的 P50/P95
- [ ] 为全量重建增加“可恢复 checkpoint”设计，减少长任务失败重来成本

### 8.4 可观测性与运行治理
- [ ] 增加 request-id 与检索链路 trace（query analysis -> doc routing -> chunk merge -> answer）
- [ ] 明确 SLO：回答成功率、拒答正确率、P95 延迟、重建成功率
- [ ] 审计日志落地轮转与保留策略（按天滚动 + 上限清理）

### 8.5 多租户/多资料库演进准备
- [ ] 设计 workspace/user 级数据隔离边界（DB、索引、审计三层）
- [ ] 补 API 版本化与 OpenAPI 文档，避免 UI/Server 迭代相互阻塞
- [ ] 预留增量状态推送通道（SSE/WebSocket）用于索引进度与告警

**Exit Criteria**
- 安全：OIDC 签名校验与 CORS allowlist 在 server 默认开启
- 稳定：关键链路有可追踪 request-id，核心 SLO 可观测
- 扩展：给出 50k+ 检索存储决策（含 benchmark 证据），并形成可执行迁移路径

## Change Log
变更日志已迁移至 `docs/PLAN_CHANGELOG.md`，便于保持计划正文聚焦执行项。

发现大量硬编码：
lib.rs:
let analysis = analyze_query("北极星生物计算PolarisBioCompute成立于");
    fn cjk_term_extractors_strip_question_tail_and_fillers() {
        let fts_terms = extract_fts_terms("新增的岗位是什么");
        assert!(fts_terms.iter().any(|term| term == "新增"));
        assert!(fts_terms.iter().any(|term| term == "岗位"));
        assert!(!fts_terms.iter().any(|term| term == "是什么"));
        let signal_terms = extract_signal_terms("这个系统是做什么的");
        assert!(signal_terms.iter().any(|term| term == "系统"));
        assert!(signal_terms.iter().any(|term| term == "这个系统"));
    }        assert!(signal_terms.iter().any(|term| term == "这个系统"));

        let phrase_terms = extract_phrase_signal_terms("新增的岗位是什么");
        assert!(phrase_terms.iter().any(|term| term == "新增岗位"));
        assert!(!phrase_terms.iter().any(|term| term == "新增的岗位是什么"));
## Architecture Overlay: Memory OS Lite

Retrieval remains the P0 reliability gate, but the product architecture has been expanded to **Local-first Verifiable Memory OS Lite**. Canonical design: [MEMORY_OS_LITE.md](./MEMORY_OS_LITE.md).

Implications for this plan:

- The main retrieval chain still owns document evidence: `document routing -> chunk retrieval -> RRF/gating -> evidence/citation`.
- Conversation/project memory can improve context, but it must be tracked as `memory_context`, not citation.
- Future regression reports must include `answer_source_mix`, `source_groups`, `failure_class`, and `context_budget_report`.
- Graph remains an explanation layer and must not affect P0 ranking metrics.
- The next external gate remains concrete: 50 questions, at least 45 answered, at least 40 correct, at least 45 citation/source-group hits.
