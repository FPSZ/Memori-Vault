# 检索精度 / 可验证记忆 —— 同类技术调研参考

> 调研日期：2026-06-02 · 用途：为 Memori-Vault 的检索精排算法（`memori-core`）与可验证记忆设计提供文献/工程参考。
> 本文只做汇总与映射，不含代码改动。每一节给出「这是什么 → 对应我们哪段代码 → 可借鉴什么」。

---

## 0. 我们当前的算法基线（一句话回顾）

```
查询分析 → 文档路由(4路 RRF) → 分块召回(strict-FTS / broad-FTS / dense 三路 RRF) → 门控打分(精度闸门, 拒答) → 来源均衡选证
```

- 融合用 **RRF**，`k=60`，候选 `K=20`，最终 `top-6`（`lib.rs:81`）。
- **dense 向量是被刻意压制的辅助信号**（RRF 权重 1.0，与 broad 同级，且 dense-only 倒扣分）。
- **没有 cross-encoder 重排序层**。
- 门控是规则打分（document_signal / lexical_grounding / coverage / multi_chunk …），阈值静态（Balanced=55）。
- 引用走「Evidence Firewall」：文档可作引用，记忆只作上下文。

下面的文献正好覆盖这几块的 SOTA 做法与我们的缺口。

---

## ★ 最高杠杆改进（先做这一个）：在召回之后加一层 cross-encoder 重排序

> **状态：核心算法已实现并通过测试（2026-06-02，memori-core）。** 见文末「实现记录」。下面论证「为什么是它」。

### 一句话
保留现有 RRF 三路召回不动，在 `merge_chunk_evidence` 产出 top-20 候选之后、`select_balanced_final_evidence` / 门控之前，**插入一个 cross-encoder 重排器**：用 `(query, chunk_content)` 成对打分重新排序，再取 top-6。即把现在的「召回即终排」升级为业界标准的 **召回 → 重排** 两段式。

### 为什么对我们提升最大（不是泛泛而谈，是针对本项目）

1. **直接命中正在回归的指标。** 我们的 P0-2 是 `repo_mixed Top-1 = 0.4773`（从 0.5682 回归）。Top-1 = 排第一的结果对不对，而 cross-encoder 重排正是 IR 文献里**最可靠的 Top-1 提升手段**——它只负责把 top-K 重新排序，让正确项落到 #1。基准数据：混合检索 + cross-encoder 重排在 100 候选时达 **0.888**，且重排是其中的主导增益项（[arXiv:2604.01733](https://arxiv.org/html/2604.01733v1)、[Superlinked](https://superlinked.com/vectorhub/articles/optimizing-rag-with-hybrid-search-reranking)）。

2. **一举绕开我们当前最可疑的两个病根。** 排查时发现的两个精度风险——
   - **dense 信号被结构性压制**（RRF 权重仅 1.0、dense-only 还倒扣 18 分，`retrieval_eval.rs:242`）：导致「语义对但用词不同」的结果永远进不了 #1，这是中文问答的高频场景；
   - **`document_rank` 一票否决**（chunk 排序里文档名次优先于 `final_score`，`retrieval.rs:601`）：第一阶段文档路由排错就被放大到最终结果。

   cross-encoder **直接对 query–chunk 语义相关性打分**，天然不依赖词法命中、也不受文档路由名次绑架。引入它之后，我们就**不必再手工微调那些脆弱的 dense 罚分和启发式 `document_reason` 优先级**——这些规则本身就是回归的高危来源。

3. **几乎零架构风险，且能复用我们刚修好的基础设施。** 重排是**纯增量**的一段：召回、门控、Evidence Firewall、引用全部不动，只在中间插一步重排序。而且它正好复用我们刚完成的**多模型运行时 + 独立端口**架构——新增第 4 个角色 `rerank`（端口 18004），与 chat/graph/embed 并列。本地有现成的 CJK 友好重排模型（`bge-reranker-v2-m3`，llama.cpp 用 `--reranking` 即可起）。

4. **协同效应最强。** 它同时是 §1（混合检索+重排）、§2（两阶段/层次化检索）的落地，也为 §3（充分上下文门控）提供更干净的输入——门控打分基于重排后的高质量 top 证据，误拒会自然下降。

### 落点（映射到现有代码，仅设计）
- **位置**：`engine_retrieve.rs::retrieve_evidence_for_analysis` 里，`merge_chunk_evidence(...)` 之后、返回 `EvidenceRetrievalResult` 之前，对 `evidence`（top-20）做一次重排，写回 `final_score` 或新增 `rerank_score` 字段。
- **排序键调整**：重排后让 `rerank_score` 成为 chunk 主排序键，`document_rank` 降为 tie-breaker（破除 §2 的「一票否决」）。
- **模型角色**：沿用 `model_settings.rs` 的角色机制，加 `rerank` 角色 + 默认端口 18004，受我们已加的 `ensure_distinct_local_endpoints` 约束。
- **降级**：重排模型不可用时跳过重排、回落到当前 RRF 排序（保证不退化、可灰度）。

### 预期收益 / 成本 / 风险
- **收益**：Top-1 / Top-k 精度显著提升（文献量级 +0.05~0.10 不等）；可删/弱化一批脆弱启发式，降低后续回归概率。
- **成本**：本地多跑一个重排模型（显存 + 每查询约 +50~150ms，只重排 top-20 故可控）；需准备 reranker 的 gguf。
- **风险**：低。纯增量、可开关、可降级；唯一新增是模型资源占用。

### 次选（若暂不愿引入第 4 个模型）
- **轻量替代**：用现有 embedding 做 **ColBERT 式 late-interaction / MaxSim** 重排，复用 embed 模型不新增进程，提升小于 cross-encoder 但零额外模型。
- **退而求其次**：先做 §3「充分上下文门控」治误拒——但它提升的是**答得出率/召回**，**不直接提升 Top-1**，所以排在重排之后。

### 实现记录（2026-06-02，已落地）

**已完成（memori-core，core 算法层）：**
- 新增第 4 个本地模型角色 `rerank`，默认端点 `localhost:18004`、默认模型名 `Qwen3-Reranker-4B`。常量与 env（`MEMORI_RERANK_ENDPOINT` / `MEMORI_RERANK_MODEL` / `MEMORI_RERANK_ENABLED`）在 `lib.rs`，配置字段在 `model_config.rs::RuntimeModelConfig`。
- 新文件 `rerank_client.rs`：`LocalRerankClient`，调用 Jina/Cohere/llama.cpp 兼容的 `POST /v1/rerank`，分数按 index 映射回输入顺序，兼容 `relevance_score`/`score` 两种字段。
- `MergedEvidence` 新增 `rerank_score: Option<f32>`；新增统一比较器 `evidence_rank_cmp`（`retrieval.rs`）：**有重排分时以其为首要键，缺失时回落到原 RRF 排序（document_rank…）**——保证未配置/不可达时行为与改造前完全一致。已替换 `merge_chunk_evidence`、`select_balanced_final_evidence`、fallback 三处排序。
- 重排步骤 `rerank_merged_evidence`（`engine_retrieve.rs`）：在 `merge_chunk_evidence` 之后对头部 ≤20 候选送重排、写回分数、按比较器重排；**未启用/服务不可达/返回异常一律静默降级到 RRF，绝不让检索失败**；`RetrievalMetrics.rerank_ms` + `query_flags` 记录 `rerank:applied/skipped`。
- `AppState.rerank_client` 接线（`Default` 从 env 解析）。
- 测试：57 个全过（54 旧 + 3 新，覆盖「有重排分按分排序」「无重排分回落 document_rank」「已重排排在未重排前」）。

**默认行为提示：** rerank 默认开启，端点 18004。若该端口没有重排服务，每次查询会快速失败并回落 RRF（localhost 拒连仅毫秒级，但会产生一条 warn 日志）。要真正吃到精度收益，需在 18004 跑一个带 `--reranking` 的 reranker（如 `bge-reranker-v2-m3`）；不想用可设 `MEMORI_RERANK_ENABLED=0`。

**Layer 2（已完成，2026-06-02）：rerank 角色全栈接入（本地专属）**
- 桌面端 DTO：`AppSettings` 加 `local_rerank_*`、`LocalModelProfileDto` 加 `rerank_endpoint/model/model_path/context_length/concurrency`（dto.rs）。
- 设置解析/保存：`resolve_model_settings` / `normalize_model_settings_payload` 贯穿 rerank；`dedupe_local_endpoints` / `ensure_distinct_local_endpoints` 扩到**四角色**端口互斥（默认 18001/18002/18003/18004）；保存写回 `commands/model.rs`。
- 运行时：`ActiveRuntimeModelSettings` + `to_runtime_model_config` 读 rerank（**仅本地 provider 启用**，远程模式自动关闭）；`apply_model_settings_to_env` 写 `MEMORI_RERANK_ENDPOINT/MODEL/ENABLED`。
- 启动：`start_local_model_role` 对 rerank 角色追加 `--reranking`；角色枚举/状态/role_* 辅助全部含 rerank（model_runtime_cmd.rs）。
- 前端（复用 `ModelCard`，无新组件）：`ModelRoleKey`/`ROLE_META`（重排模型卡，端口 18004，`Qwen3-Reranker-4B`，ListOrdered 图标）/`roleEndpoint`/`roleModel`/`modelPathForRole` 加 rerank（modelUtils.ts + models-helpers.ts）；`ModelsTab` 角色数组本地追加 rerank（远程不显示）、`expandedRoles`/`handlePickFile`/校验贯穿；`types.ts`/`useAppModel.ts`/`api/desktop.ts` 角色 union 加 rerank；`DEFAULT_MODEL_SETTINGS` 补 rerank 字段。
- 校验：`cargo check --workspace` 通过、`cargo test -p memori-core` 57 全过、`tsc --noEmit` 通过。

**Layer 3（已完成，2026-06-03）：轻量默认模型 + 一键下载**
- 默认重排模型由 `Qwen3-Reranker-4B`（4B / GGUF ~8GB / 慢 / llama.cpp 转换易出近零分坑）改为 **`gte-multilingual-reranker-base`**（~3 亿参数 / FP16 GGUF ~590MB / 编码器架构约 10× 速度 / 多语言 SOTA / `--reranking` 兼容成熟）。常量 `DEFAULT_RERANK_MODEL_QWEN3`→`DEFAULT_RERANK_MODEL_GTE`（memori-core lib.rs）及全部引用（model_config.rs、desktop lib.rs/model_settings.rs、server model_runtime.rs）；前端默认值（app-helpers.ts、modelUtils.ts/models-helpers.ts 的 `ROLE_META`）。
- 一键下载：新增 Tauri 命令 `download_rerank_model(app)`（commands/model.rs），reqwest 流式（`response.chunk()` 循环，无需 `stream` feature）从 HF `gpustack/gte-multilingual-reranker-base-GGUF` 拉取 FP16 GGUF 到 `<models_root|配置目录/Memori-Vault/models>/gte-multilingual-reranker-base/`，`.part`→rename 原子落地，按 ~4MB 步进 emit `rerank_model_download_progress` 进度事件，已存在则跳过。前端 `ModelCard` 在「浏览」右侧加「下载」按钮（仅 rerank，带进度百分比），`ModelsTab.handleDownloadRerankModel` 监听进度事件、完成后自动填好 `rerank_model_path`/`rerank_model`/`models_root`。
- 顺带修复：`set_model_settings`（commands/model.rs）此前**漏写 5 个 `local_rerank_*` 持久化字段**，导致保存后重启丢失重排配置（尤其 model_path）；已补全。
- 校验：`cargo check --workspace` 通过、`cargo test -p memori-core` 57 全过、`tsc --noEmit` 通过。

**仍待办：**
- 回归集量化重排前后的 `repo_mixed Top-1` 增益（对接 §8）。

---

## 1. 混合检索 + RRF 融合（对应：文档路由 / 三路分块融合）

**结论**：我们的 RRF(k=60) 与 BM25+dense 混合是业界标准做法，原始 k=60 的推荐与我们一致。但 SOTA 普遍在 RRF **之后再加一层 cross-encoder 重排**，这是我们目前最大的工程缺口。

| 资源 | 类型 | 要点 |
|---|---|---|
| Cormack et al., *Reciprocal Rank Fusion outperforms Condorcet…* (2009) | 论文 | RRF 原始论文，`k=60` 的来源 |
| [M3-Embedding / BGE-M3 (arXiv:2402.03216)](https://arxiv.org/abs/2402.03216) | 论文 | 单模型同时输出 dense + sparse + multi-vector，三者协同 > 任一单独；**多语言/CJK 强** |
| [From BM25 to Corrective RAG (arXiv:2604.01733)](https://arxiv.org/html/2604.01733v1) | 论文(2026) | 系统对比 RRF vs Convex Combination；混合+cross-encoder 重排在 100 候选时显著最优（0.888） |
| [Optimizing RAG with Hybrid Search & Reranking (Superlinked)](https://superlinked.com/vectorhub/articles/optimizing-rag-with-hybrid-search-reranking) | 工程 | 混合检索+重排的落地参数与权衡 |

**可借鉴**：
- 把现在「broad/dense 平权 + 启发式 document_reason 排序」升级为 **召回(RRF) → 重排(cross-encoder)** 两段式，重排只跑 top-20→top-6，成本可控。
- 评估是否引入 BGE-M3 这类**单模型同时产 dense+sparse** 的 embedding，天然适配我们「词法+向量」双轨且 CJK 友好（见 §7）。

---

## 2. 两阶段 / 层次化检索（对应：文档路由 → 分块召回）

**结论**：我们「先选文档、再在文档内选 chunk」的设计，正是学界的 **Dense Hierarchical Retrieval**，方向正确。风险在于第一阶段排错会被第二阶段放大（我们代码里 `document_rank` 优先于 chunk `final_score`）。

| 资源 | 类型 | 要点 |
|---|---|---|
| [Dense Hierarchical Retrieval, DHR (arXiv:2110.15439)](https://arxiv.org/pdf/2110.15439) | 论文 | 文档级检索器先定位文档，再用段落级检索器选段落；宏观语义+微观语义结合 |
| RAPTOR (递归摘要树) | 论文 | 对 chunk 递归聚类+摘要，建多层树，兼顾局部细节与全局主题 |
| [Hierarchical Re-ranker Retriever, HRR (arXiv:2503.02401)](https://arxiv.org/pdf/2503.02401) | 论文 | 层次化重排 |
| [Two-Stage Retrieval 综述](https://www.emergentmind.com/topics/two-stage-retrieval-method) | 概念 | 候选选择与排序解耦：轻量召回 + 重型重排 |

**可借鉴**：
- 第一阶段文档排错会一票否决正确 chunk —— 考虑让 chunk 的强信号（高覆盖率/strict 多命中）能**反向提升或召回**其文档，而非被 `document_rank` 锁死。
- RAPTOR 的「摘要树」思路可与我们的 graph extraction 互补，用于回答跨文档/全局型问题（见 §6）。

---

## 3. 拒答 / 答案门控 / 充分上下文（对应：`retrieval_eval.rs` 门控打分）⭐核心

**结论**：这是与我们「门控闸门」最直接对应的研究方向，也是我们 P1 里「gating false negative（R19/R35/R42 误拒）」的解药来源。学界共识：**"上下文不足就拒答" 太粗暴**，应结合「充分上下文信号 + 模型自评置信度」做选择性回答。

| 资源 | 类型 | 要点 |
|---|---|---|
| [Sufficient Context (arXiv:2411.06037, Google)](https://arxiv.org/pdf/2411.06037) | 论文 | 提出"充分上下文"这一新视角；结合充分性+自评置信度做选择性生成，正确率 +2~10% |
| [Controlling Risk of RAG: Counterfactual Prompting (EMNLP 2024 Findings)](https://aclanthology.org/2024.findings-emnlp.133.pdf) | 论文 | 两大置信因子：检索质量 + 利用方式；反事实提示引导模型自评是否该答 |
| [Do RALMs Know When They Don't Know? (arXiv:2509.01476)](https://arxiv.org/html/2509.01476v3) | 论文 | 系统评测 RAG 的拒答能力 |
| [RefusalBench (arXiv:2510.10390)](https://arxiv.org/html/2510.10390v1) | 基准 | 生成式评测「选择性拒答」 |
| [Know Your Limits: Abstention 综述](https://www.researchgate.net/publication/393331033) | 综述 | 拒答的全景分类（含 AbstentionBench、GaRAGe） |

**可借鉴**：
- 我们的门控目前是**纯检索侧规则分**（无模型自评）。可补一路「**充分上下文判定**」：在生成前用一次轻量判断「现有证据是否足以回答」，与现有规则分做与/或组合，减少误拒。
- 阈值从静态 55 → **随查询类型/候选规模自适应**（短查询 coverage 分母小易虚高、长查询易触发 dense-only 罚分，当前是已知偏差）。
- 建一个我们自己的 **RefusalBench 式拒答评测集**，把 R19/R35/R42 这类「可答却被拒」case 固化为回归用例（对接 §8 的 CI 缺口）。

---

## 4. 可验证引用 / 归因式问答（对应：Evidence Firewall / Citation/Trust Panel）

**结论**：我们「文档可引用、记忆只作上下文」的 Evidence Firewall 与归因式 LLM（Attributed QA）目标一致。学界已有成熟评测（ALCE）和「先选引文再生成」的训练框架（FRONT），可直接用来量化我们的引用质量。

| 资源 | 类型 | 要点 |
|---|---|---|
| [ALCE: Enabling LLMs to Generate Text with Citations (EMNLP 2023)](https://aclanthology.org/2023.emnlp-main.398.pdf) | 论文/基准 | 首个引用自动评测基准；三维度：流畅性 / 正确性 / **引用质量**（citation precision & recall） |
| [Attributed QA: Evaluation and Modeling (Bohnet et al.)](https://www.semanticscholar.org/paper/05d77715d49714506a920f26c5432b92078cd37c) | 论文 | 归因式 LLM 的可复现评测框架 |
| [FRONT: Fine-Grained Grounded Citations (arXiv:2408.04568)](https://arxiv.org/pdf/2408.04568) | 论文 | 两阶段：先从来源选支撑引文，再 condition 在引文上生成 → 细粒度 grounding |
| [Learning to Generate Answers with Citations via Factual Consistency (Amazon)](https://assets.amazon.science/b9/c2/2a961e5849c8b3d2b5037920a35e/learning-to-generate-answers-with-citations-via-factual-consistency-models.pdf) | 论文 | 用事实一致性模型做引用监督 |

**可借鉴**：
- 用 **ALCE 的 citation precision/recall** 指标评测我们的 Trust/Citation Panel，量化「引用是否真的支撑了答案」。
- FRONT 的「**先选引文再生成**」与我们「先 retrieve evidence 再 answer」流程同构，可借其细粒度 grounding 提示词降低幻觉。

---

## 5. 自反思 / 纠错 / 自适应检索（对应：未来的 query 分析与重试逻辑）

**结论**：我们已有 `gating_retry_on_refusal`（拒答后重试）和 query_family 分流，这是 Self-RAG / Adaptive-RAG 思路的雏形。可进一步引入「检索质量自评 → 决定是否重检/纠错」。

| 资源 | 类型 | 要点 |
|---|---|---|
| [Self-RAG (arXiv:2310.11511)](https://arxiv.org/pdf/2310.11511) | 论文 | 用 reflection token 让模型按需检索、对检索结果与自身输出自评 |
| [CRAG: Corrective RAG (arXiv:2401.15884)](https://arxiv.org/pdf/2401.15884) | 论文 | 对检索结果自评分级（正确/模糊/错误），差则触发纠正（如改写+重检） |
| [Adaptive-RAG](https://humanloop.com/blog/rag-architectures) | 方法 | 按查询复杂度路由不同检索策略（简单直答 / 复杂多跳） |

**可借鉴**：
- CRAG 的「**检索质量分级器**」可替换/增强我们门控里的启发式打分。
- Adaptive-RAG 的「按复杂度路由」与我们 `query_family` + `compound_query` 拆分思路一致，可统一为显式的查询复杂度路由层。

---

## 6. 图谱增强检索（对应：`graph_extractor` / GraphRAG）

**结论**：我们的 graph extraction 与 Microsoft GraphRAG 同源。GraphRAG 的核心价值是回答**全局型/跨文档**问题（"主要主题是什么"），这正是纯 chunk 检索的盲区。

| 资源 | 类型 | 要点 |
|---|---|---|
| [From Local to Global: GraphRAG (arXiv:2404.16130)](https://arxiv.org/abs/2404.16130) | 论文 | LLM 抽知识图谱 + 社区摘要；global search 回答全局 sensemaking 问题 |
| [DRIFT Search (Microsoft)](https://www.microsoft.com/en-us/research/blog/introducing-drift-search-combining-global-and-local-search-methods-to-improve-quality-and-efficiency/) | 工程 | 结合 global + local search，兼顾质量与效率 |
| [GraphSearch (arXiv:2509.22009)](https://arxiv.org/pdf/2509.22009) | 论文 | Agentic 深度图检索工作流 |

**可借鉴**：
- 区分 **local（具体事实，走 chunk 检索）vs global（主题/汇总，走图社区摘要）** 两类查询，与我们 query_family 融合。
- GraphRAG 社区摘要 + RAPTOR 摘要树（§2）可合并为我们的「全局问答」能力。

---

## 7. 多语言 / CJK 检索（对应：`query.rs` 的 CJK 短语处理、向量模型选型）

**结论**：我们目前在 query 分析层做了大量 CJK 启发式（短语切分、噪声词表、问句后缀剥离）。BGE-M3 这类模型把多语言/CJK 能力下沉到 embedding 本身，可减少启发式负担。

| 资源 | 类型 | 要点 |
|---|---|---|
| [BGE-M3 (arXiv:2402.03216)](https://arxiv.org/abs/2402.03216) | 论文/模型 | 100+ 语言；同时 dense+sparse+multi-vector；最长 8192 token；CJK 友好 |
| [Qwen3-Embedding vs BGE-M3 对比](https://medium.com/@mrAryanKumar/comparative-analysis-of-qwen-3-and-bge-m3-embedding-models-for-multilingual-information-retrieval-72c0e6895413) | 工程 | 多语言检索两个主力 embedding 的对比（我们 embed 默认 Qwen3-Embedding-4B） |

**可借鉴**：
- 我们 embed 角色默认就是 `Qwen3-Embedding-4B`，与上文对比文章对象一致；可据此对比是否换/补 BGE-M3 作为 sparse+dense 一体方案。
- BGE-M3 的 sparse 输出可直接喂我们的词法通道，省去部分 FTS 工程。

---

## 8. 评测与回归防护（对应：P0-2 检索回归、P1 无 CI 守卫）⭐工程

**结论**：我们已有 `retrieval_eval.rs` 但语料过小（6+11 文档）、无 CI 守卫。上面每个方向都自带成熟基准，可拿来扩充我们的回归集。

可直接借用/对标的基准：
- **检索质量**：BEIR、MTEB（embedding 选型）
- **引用质量**：ALCE（precision/recall）
- **拒答能力**：RefusalBench、AbstentionBench、GaRAGe
- **充分上下文**：Sufficient Context 的 autorater 方法

**可借鉴**：
- 把「拒答误判 case（R19/R35/R42）」「引用是否支撑答案」固化为带指标阈值的回归用例，挂 CI，防止 `repo_mixed Top-1` 这类静默回归（直接对接 [IMPROVEMENTS.md](./IMPROVEMENTS.md) 的 P0-2 / P1）。

---

## 9. 长期 / 分层记忆（对应：STM/MTM/LTM、记忆生命周期、MCP agent memory）

**结论**：我们的分层记忆 + 记忆生命周期日志，与 MemGPT/Mem0 系列同属「Agentic Memory」赛道。Mem0 的「抽取事实 → 巩固 → 检索」与混合 vector+graph 存储，和我们架构几乎一一对应。

| 资源 | 类型 | 要点 |
|---|---|---|
| MemGPT | 论文 | 仿 OS 内存分层：Main Memory(RAM) + External Memory(disk) |
| [Mem0 (arXiv:2504.19413)](https://arxiv.org/pdf/2504.19413) | 论文 | LLM 抽取事实→巩固→检索；**hybrid vector + graph 存储**；生产级记忆层 |
| A-MEM | 论文 | 决策驱动的记忆操作（agentic memory） |
| [Hierarchical Memory / H-MEM (arXiv:2507.22925)](https://arxiv.org/pdf/2507.22925) | 论文 | 按语义抽象度多级组织/更新记忆 |
| [HiMem (arXiv:2601.06377)](https://arxiv.org/pdf/2601.06377) · [Membox (arXiv:2601.03785)](https://arxiv.org/pdf/2601.03785) · [MemMachine (arXiv:2604.04853)](https://arxiv.org/html/2604.04853v1) | 论文(2026) | 长程对话记忆构建/检索/动态更新；主题连续性；ground-truth 保真 |

**可借鉴**：
- Mem0 的 **hybrid vector+graph 记忆存储** 正是我们 storage 已有的方向，可对标其「事实抽取→巩固去重」流程优化我们的记忆生命周期。
- H-MEM 的「按语义抽象度分层」可指导 STM/MTM/LTM 的晋升/降级规则。

---

## 10. 优先借鉴清单（按 ROI 排序，对接现有 P0/P1）

| 优先级 | 借鉴项 | 来源 | 解决我们的 |
|---|---|---|---|
| **P0** | 召回后加 **cross-encoder 重排**（top-20→6） | §1, §2 | P0-2 检索回归、dense 被压制 |
| **P0** | 门控补「**充分上下文**」自评，阈值自适应 | §3 | P1 gating 误拒 (R19/R35/R42) |
| **P0** | 扩充回归集 + 挂 **CI 指标守卫**（ALCE/RefusalBench 式） | §8 | P0-2、P1 无 CI 守卫 |
| P1 | 评估 **BGE-M3** 作为 dense+sparse 一体 embedding | §1, §7 | dense 信号弱、CJK 启发式过重 |
| P1 | chunk 强信号可**反向召回/提升文档**，破除 `document_rank` 一票否决 | §2 | 文档路由排错放大 |
| P2 | CRAG 检索质量分级器替换部分启发式打分 | §5 | 门控鲁棒性 |
| P2 | GraphRAG/RAPTOR 全局问答（local vs global 路由） | §6, §2 | 跨文档/汇总型问题盲区 |
| P2 | 用 ALCE 指标量化 Evidence Firewall 引用质量 | §4 | 引用可信度无量化 |

---

### 附：检索精度问题与文献的对应速查
- **dense 语义召回弱（中文同义不同词被拒）** → §1 重排 / §3 充分上下文 / §7 BGE-M3
- **门控误拒可答问题** → §3 全套
- **文档路由排错被放大** → §2 DHR / chunk 反向召回
- **覆盖率纯子串匹配噪声** → §7 更好的 embedding + sparse / §1 重排兜底
- **无法回答全局/汇总型问题** → §6 GraphRAG + §2 RAPTOR
