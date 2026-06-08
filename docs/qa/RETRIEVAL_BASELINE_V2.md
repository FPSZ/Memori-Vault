# 检索评测 v2 困难基准

> 一条**全新的、更难的**基准线，与 v1（`Memory_Test/` 100 份 / 100 题、v1.2.0–v1.5.0 那套对比图）**不可直接比较**。v1 历史数据与对比图原样保留；v2 从这里重新起跑。

## 语料与套件
- 语料：`Memory_Test_V2/` 共 **506 份**多体裁内部资料 = **280 信号**（40 虚构项目 × 7 体裁）+ **220 干扰**（同公司、话题相邻、不含任何题目答案，做"大海捞针"）+ **6 特殊**（长文 / 内嵌图片 / 扫描件）。
- 格式：md / txt / docx / pdf / **pptx / xlsx**（OpenXML）+ **doc / ppt / xls**（OLE2 老格式）。解析全部自实现：pptx/xlsx 走 zip+quick-xml；xls 走 `calamine`；doc 走 `cfb`+FIB/CLX piece-table；ppt 走 `cfb`+记录树 TextChars/TextBytes atom。9 种格式端到端可索引。
- 难度机制：① 事实埋进散文（无 `核心事实：` 标签）；② 时效冲突取最新（制度写旧值、复盘 pdf 修正为现值）；③ 跨文档多跳（代号/事实分散在不同体裁）；④ 表格/幻灯片结构定位（答案=某单元格 / 某页标题+正文）。
- 套件：`docs/qa/retrieval_regression_suite_v2.json`，**108 题 = 92 答 + 16 拒**，覆盖 **17 个能力维度**（含长文检索、图文可抽取、图片/扫描丢失）。
- 生成器：`Memory_Test/generate_corpus_v2.py`（语料 + manifest）、`scripts/generate-memory-test-regression-suite-v2.py`（套件，消费 manifest，带 clue 在/不在 自校验）。

## 运行配置
```
cargo run -q -p memori-core --example retrieval_regression -- \
  --mode live_embedding --profile full_live \
  --watch-root Memory_Test_V2 --index-all \
  --suite docs/qa/retrieval_regression_suite_v2.json \
  --db-path target/retrieval-regression/v2run.db --max-index-prep-secs 1200 --max-case-secs 60
```
- `--index-all`：把 watch-root 下全部 506 份支持格式都进索引（干扰库做大海捞针），而非只索引题目目标文档。
- 嵌入：Qwen3-Embedding-4B @ :18003；重排：bge-reranker-v2-m3 @ :18004（gte 的 gguf 为 `new` 架构，当前 llama-server 构建不支持，本轮改用 bge 多语重排）。
- **索引 506 份耗时 ≈ 128.7s**（含一份 3 万字长文）。

## 检索结果（report：`docs/qa/retrieval_regression_v2_report.json`）

| 指标 | v2（506文档/108题） |
| --- | ---: |
| 整体 reject_correct | **0.833** |
| top1 文档命中 | **0.696** |
| top3 文档召回 | **0.913** |
| top1 片段命中 | **0.750** |
| top5 片段召回 | **0.957** |
| 片段 MRR | **0.830** |
| 重排应用率 | 0.954 |
| 引用有效率 | 1.000 |

解读：干扰库 + 散文埋点把 **top1 文档精度压到 0.696**（难度生效），但 **top3 召回 0.913**——正确文档多数仍进 top3。相对早期 500/100 跑（top3 0.952）小幅回落，主因是新增的 4 道"图片/扫描丢失"题（事实在像素里，注定 miss）和 2 道长文题（埋点深、被 gating 拒）拉低了答案题分母。

## 各能力维度（含每题平均耗时）

| 能力维度 | 题数 | 正确(答/拒) | top1文档 | top3文档 | top5片段 | 平均耗时 |
|---|--:|--:|--:|--:|--:|--:|
| 直问-散文事实 | 12 | 12 | 0 | 9 | 12 | 1925ms |
| 改写/语义召回 | 10 | 10 | 3 | 10 | 10 | 1939ms |
| 反常识/抗参数知识 | 8 | 8 | 8 | 8 | 8 | 1498ms |
| 代号/ID/别名检索 | 6 | 6 | 6 | 6 | 6 | 1263ms |
| 相似代号防串 | 6 | 6 | 5 | 6 | 6 | 3252ms |
| 跨文档多跳 | 8 | 8 | 8 | 8 | 8 | 4154ms |
| 表格单元格定位(xlsx) | 8 | 7 | 7 | 8 | 8 | 1520ms |
| 幻灯片跨页(pptx) | 6 | 6 | 6 | 6 | 6 | 1607ms |
| 时效冲突取最新 | 8 | 8 | 8 | 8 | 8 | 1856ms |
| 多格式抽取 | 6 | 1 | 2 | 3 | 6 | 1774ms |
| 口语/错别字/省略 | 4 | 4 | 4 | 4 | 4 | 1349ms |
| 长难句/多条件 | 2 | 2 | 2 | 2 | 2 | **7720ms** |
| 长文检索(几万字) | 2 | 0 | 2 | 2 | 2 | 1391ms |
| 图文-可抽取(caption/正文) | 2 | 2 | 2 | 2 | 2 | 1246ms |
| 图片/扫描丢失(预期miss) | 4 | 0 | 1 | 2 | 0 | 1129ms |
| refuse-库中无此事实 | 8 | 4 | — | — | — | 2005ms |
| refuse-越权/注入/常识外推 | 8 | 6 | — | — | — | **751ms** |

> 答案题"正确"= 系统选择作答（未误拒）；harness 不对 LLM 答案文本判分，事实正确性由 top-k 命中间接反映。

## 每题耗时（性能指标）
- **平均每题 1989ms**（注：这是**检索+判答**耗时，harness 不调答案生成 LLM，不含 LLM 写答案的时间）。
- 最快 **44ms**（`V093` "OpenAI CEO 是谁"——意图拦截在重检索前短路）；最慢 **8197ms**（`V084` 长难句多条件复合查询）。
- 耗时结构：**复合/多条件查询最贵**（长难句 avg 7.7s、跨文档多跳 4.2s、相似代号防串 3.3s——都会扇出成多个子查询并各自重排）；**注入/越权拒答最快**（avg 0.75s，硬拦短路）；普通单事实题 ~1.2–2.0s；长文检索查询时**并不慢**（1.4s，成本在索引期）。
- **记录（非指标）：最慢题 `V084`（复合多条件，8.2s）/ 最快题 `V093`（注入拒答，44ms）。** harness 现已把 `case_total_ms` 写进每条 case，summary 含 `avg/min/max_case_ms` 与 `slowest/fastest_case_id`。

## 图谱构建速度（时间 / 文字数比）
bench：`cargo run -p memori-core --example graph_bench -- <files>`（对每个 chunk 调一次图谱 LLM=Qwen3-8B@:18002 抽实体/关系并计时）。

| 文档 | 字数 | chunk数 | 总耗时 | ms/字 | ms/chunk |
|---|--:|--:|--:|--:|--:|
| 纪要(短) | 263 | 1 | 10.5s | 39.8 | 10477 |
| 制度(中) | 504 | 3 | 32.7s | 64.9 | 10904 |
| 云梯手册(长文) | 25718 | 33 | **33 分钟** | 77.0 | **59999** |

**结论：图谱构建瓶颈 = 每 chunk 一次 8B LLM 调用，且耗时随 chunk 大小增长**——小/半块 ≈10s，满 1000 字块 **≈60s**。粗略 **≈1 分钟 / 千字**（满密度长文）。一份 **2.5 万字文档 ≈ 33 分钟**图谱构建；3 万字长文 ≈ 半小时。这量化了"长文对图谱极贵"，也正是下面护栏要挡的场景。

## 图谱构建速度优化（本轮，实测 4–5×）
研究方向：图谱构建 = `N_chunk × 单调用延迟 / 并发`，而单调用延迟由**解码输出 token 数**主导（自回归，~线性）。对照 GraphRAG / LightRAG 文献，安全（不降质量）的杠杆是 ① 压输出 token、② 杜绝解析失败重试、③ 提并发（continuous batching）。落地四项（均不动召回质量）：

1. **精简输出 schema**（`graph_extractor.rs`）：模型只产 `name/type/desc?`，边**按 name 互引**；`id` 改为 Rust 端按名称稳定派生（`slug`，CJK 保留）。既省 token，又消除"节点 id ≠ 边引用 id"这一最常见抽取错误（边↔节点一致性反而更好）。`desc` 保留但要求一句话以内。
2. **约束 + 封顶解码**：请求带 `response_format=json_object` + `max_tokens=1536`（env `MEMORI_GRAPH_MAX_TOKENS` 可调）。前者去掉 JSON 外的散文/markdown，后者**斩断 runaway 解码**（旧版无上限，密块曾跑到 1174+ token / 40s）。
3. **<think> 兜底剥离**：解析前去掉可能泄漏的思考块，避免一次思考前缀就触发解析失败→重试（旧版重试是 3× 浪费）。
4. **默认并发 2→4**（`indexing_graph.rs`）：图谱解码受限于 LLM，llama.cpp `--parallel N --cont-batching` 下并发槽近似线性提吞吐；`MEMORI_GRAPH_CONCURRENCY` 对齐服务端 `--parallel`。

**实测（Qwen3-8B-Q4_K_M @ :18002，HIP GPU，长文 `special_001` 的 8 个满 1000 字密块）：**

| 配置 | 8 块总耗时 | 每块 | 说明 |
|---|--:|--:|---|
| 旧（verbose schema, 并发2） | 76.3s | 9.53s | 含 runaway 长尾块（40s/块） |
| 新（slim+cap+json, 并发4） | **15.4s** | **1.92s** | **端到端 4.96×（快 80%）** |

分解：同并发=2 下，仅 schema+cap+json 即 **3.5×**（76.3→21.6s，主因封顶斩断 runaway 长尾）；并发 2→4 再叠 **1.41×**（21.6→15.3s）。短/普通块（`sig_001/sig_015` 单块）输出 token 减 **27%**、墙钟减 **26%**，解析成功率不变（2/2）。质量抽查：`sig_001` 仍抽出 14 节点 / 11 边（极光账本/项目·含代号 AUR-17、林知远/负责人、对接客户等），结构完整。

**影响：** 长文 2.5 万字图谱构建从 ~33 分钟 → **~7 分钟**量级；普通文档同步变快。bench 工具：`scripts/graph_ab_bench.py`（schema A/B）、`graph_concurrency_bench.py`（并发吞吐）、`graph_combined_bench.py`（端到端前后对比）。
> 服务端建议：`llama-server ... --parallel 4 --cont-batching`，并令 `MEMORI_GRAPH_CONCURRENCY` 与之一致。

## 长文 / 图片 / 扫描件 的真实处理结论（专门样本实测）
- **长文（3 万字）**：索引/分块正常（切成 ~35 个 ≤1000 字块），检索能把长文**召回到 doc rank 1**；但埋在深处的事实其片段只排到 rank 2–4，两道长文题（`V101/V102`）最终被 **gating 判拒**——长文通过"埋点深 + gating 保守"双重打击降低可答率。图谱构建则极贵（见上）。
- **图片内容**：`extract_*` 只取文本，**图片一律忽略、全链路无 OCR**。实测：
  - 图说明/正文里的事实（`V103/V104`）→ 正常作答 ✓。
  - 事实只画在图里（`V105` 晨曦回滚阈值、`V108` 白川预算图）→ 片段 rank=None，作答被拒；纯图 docx（`V106` 暮山）→ 文档都召不回。
  - **扫描件 PDF**（`V107` 苍岭）→ lopdf 抽 0 字，文档完全不可见。
  - 即这 4 道"图片/扫描"题 **0/4 可答**——坐实"图片/扫描内容不可检索"的能力缺口（如需可检索须接 OCR，单列大功能）。

## 索引护栏（本轮新增）
`memori-core/src/indexing.rs`：
- `MAX_INDEX_FILE_BYTES = 50 MB`：超大文件直接跳过（warn），防内存/时间失控。
- `MAX_CHUNKS_PER_DOC = 400`：每文档 chunk 数封顶后截断（warn），防一份病态超长文把嵌入+图谱队列打爆（按上面 ≈1 分钟/千字，400 块≈40 万字已是 ~6 小时图谱，硬上限兜底）。

## 失败分析（改进杠杆，非套件 bug）
- **A. 答案题被误拒（检索正确、gating 过保守）**：`多格式抽取` 仅 1/6 作答、`长文检索` 0/2、`xlsx` 1 题——文档/片段命中 rank 1–2 但 gating 打分 < 阈值 55 走 `score_below_threshold`。集中在"单事实/低词法覆盖"证据，与 v1 同源（可由 coverage / rerank 置信度放行路径再调）。
- **B. 拒答题被泄露作答（困难语料触发误放行）**：`refuse-库中无此事实` 仅 4/8。诱饵代号 / 不存在属性触发 `identifier_grounded_release` / `rerank_confident_release` / 复合查询 `compound_partial_release`（gate=0 绕过）。**复合查询路径是当前拒答安全的主要缺口，PII/越权类最该优先修。**
- **C. 图片/扫描（B 类预期 miss）**：非 bug，是无 OCR 的能力边界，已用 4 道题固定记录。

## 重排模型 A/B（本轮，同 embed/同语料/同代码，仅换 :18004 重排服务）
- **bge-reranker-v2-m3（现默认）**：Top-1 文档 69.6% / Top-3 文档 91.3% / Top-1 chunk 75.0% / Top-5 chunk 95.7% / chunk MRR 0.8301 / 拒答 83.3% / 平均检索 ≈1.5s。平滑 logit（−7.5~8.0），与现有"裸分 min-max 融合 + gating 阈值"调校天然兼容。
- **Qwen3-Reranker-0.6B（已否决）**：Top-1 文档 46.7%（−22.9pp）/ Top-3 文档 71.7% / 拒答 52.8%（−30.5pp）/ 平均检索 3.4s。**Top-5 chunk 仅 −3.3pp 说明候选召回正常，是重排把正确文档往下压**——经 llama.cpp 输出近二值相关概率（0.0001~1.0，87% 的 top 分 >0.9），中段无法区分、gating 被满屏"1.0"骗到过度放行。"已召回却排不进 top3"错排率从 bge 的 7% 飙到 26%。要可用须重调融合权重 + 重标定 gating 阈值，工程量大、收益存疑。
- **gte-multilingual-reranker-base（已不可用）**：gguf 为 `new` 架构，当前 llama-server 构建报 `unknown model architecture: 'new'` 加载失败，三个本地 build 均不支持；v2 从未在 gte 上测过。
- **代码默认对齐**：`memori-core` 常量 `DEFAULT_RERANK_MODEL_GTE` → `DEFAULT_RERANK_MODEL_BGE = "bge-reranker-v2-m3"`，桌面/服务端默认值、UI 预设、一键下载（`gpustack/bge-reranker-v2-m3-GGUF` Q4_K_M, ~390MB）同步更新。

## 下一步杠杆（仅记录，不在本轮）
1. gating 对"单事实低词法覆盖"证据的放行（A 类）。
2. 复合查询路径补 gating 与越权/PII 拦截（B 类）。
3. 诱饵代号 / 不存在属性的拒答硬化。
4. 长文：分块/gating 对深埋事实的处理（长文题 0/2）。
5. OCR：图片/扫描件可检索（C 类，大功能，需接 tesseract 或视觉模型）。
6. 重排已切到 bge-reranker-v2-m3（见上节 A/B）。若日后要上 Qwen3-Reranker，需先为其近二值分数重调融合权重 + 重标定 gating 阈值，再复测。
