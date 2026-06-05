# Gating 误拒修复方案（第 N+1 轮 · 给 GPT 实施）

> 目标：把"目标文档已正确召回、却被软 gate 拒答"的回答题救回来。
> 数据基准：`target/retrieval-regression/live_embedding-full_live-1780582124/report.json`（最新一次 100 题跑）。

## 0. 现状与根因（先读，决定改法的安全性）

上一轮 P0–P3 落地后最新跑的拆解：

| 维度 | 基线 #11 | 最新 #2124 |
|---|---|---|
| 拒答题正确拒答 | 4/12 | **11/12** ✅ |
| 回答题正确作答 | 44/88 | **40/88** ❌ |
| 整体 reject_correct | 0.48 | 0.51 |

`reject_correct` 是混合指标（拒答题看"是否拒"，回答题看"是否答"）。**P3 把拒答救起来了,但 88 道回答题里 48 道被误拒,其中 36 道目标文档已在 top3,35 道死因是 `score_below_threshold`。**

### 关键事实 A —— 瓶颈在软 gate 打分，不在召回
35 道 `score_below_threshold` 的误拒题里，**绝大多数 `chunk_hit_rank=1`、`top1_chunk_hit=true`**：黄金 chunk 已经被排到第一，gate 却给不到 Balanced 阈值 55。检索是好的，是评分函数低估了它们。

### 关键事实 B —— 安全垫已就位，可以放心抬分
最新跑里 12 道拒答题的判定路径：
- C089–C093（不存在的代号 NOVA-404/OBS-88/MKT-99…）→ `entity_not_grounded`（**硬拦，在软 gate 之前**）
- C095/C096/C098/C099/C100（外部事实 / 密钥 / 注入）→ `intent_blocked`（**硬拦**）
- C097（外部汇率）→ `score_below_threshold`（唯一一道走软 gate 分的拒答题）
- C094（"银杏表是否规定了12项"——实体存在但所问属性不存在）→ 已被误答（与本方案无关，见 §4）

→ **10/12 拒答题由硬拦在软 gate 上游守住。** 因此放宽软 gate 打分**不会**放跑这些拒答题。唯一走软 gate 的 C097 没有可落地的业务代号、coverage 极低，本方案的定向抬分不会碰到它。

### 关键事实 C —— coverage 分项对"精确代号查询"存在结构性低估
软 gate 打分见 `retrieval_eval.rs::decide_with_profile`（:163 起）。coverage 分项（:232-241）按**绝对命中数**分档：

```
distinct_hits >= 3 && coverage >= 0.65  -> 26
distinct_hits >= 2 && coverage >= 0.4   -> 22
distinct_hits >= 1 && coverage >= 0.2   -> 8
else                                    -> 0
```

P2 把"唯一事实卡/核心事实/责任人…"清进噪声词后，一条精确代号查询（"赤松预算的唯一事实卡核心事实是什么"）的 `support_terms` 往往**只剩 1 个判别性词**（代号本身）。即便它 100% 命中（coverage=1.0、distinct_hits=1），也只能落到 `>=1` 档拿 **8 分**——永远够不到 22/26 档（需要 ≥2/≥3 个不同词）。

一道典型误拒题的分项估算（document_signal 10 + lexical 16 + coverage **8** + multi_chunk 10 + cross_source 8 + lookup 6 = 58…或更低）在 55 线上下剧烈抖动，coverage 这 8 分的天花板正是把它们压到线下的主因。

## 1. 改动（按优先级；G1 最高杠杆，G1+G2 应足够）

全部改 `memori-core/src/retrieval_eval.rs`，**不碰** P3 的硬拦逻辑、不动 Balanced=55 阈值（降阈值是钝器，会波及 C097 类软 gate 拒答与未来的临界 ungrounded 题）。

### G1 — 修 coverage 对少词查询的低估（principled，必做）
`decide_with_profile` coverage 分项（:232-241）增加一档"高比例覆盖"判定：当 support_terms 几乎全部命中时，按比例给高分，不受绝对数 ≥2/≥3 的限制。

```rust
breakdown.coverage =
    if inputs.top_doc_distinct_term_hits >= 3 && inputs.top_doc_term_coverage >= 0.65 {
        26
    } else if inputs.top_doc_distinct_term_hits >= 2 && inputs.top_doc_term_coverage >= 0.4 {
        22
    } else if inputs.top_doc_distinct_term_hits >= 1 && inputs.top_doc_term_coverage >= 0.8 {
        // 新增：少词精确查询全覆盖（典型：单代号 query，coverage=1.0）
        // 给到接近满档，修复"判别性代号 100% 命中却只拿 8 分"的结构性低估
        20
    } else if inputs.top_doc_distinct_term_hits >= 1 && inputs.top_doc_term_coverage >= 0.2 {
        8
    } else {
        0
    };
```

> 取 20 而非 26：保留与"多词高覆盖"档的梯度。单词全覆盖（+12 比原 8 分）通常足以把 58±抖动的题稳定推过 55。落地后看 #N+1 数据再决定是否调到 22。

### G2 — 实体已落地放行路径（targeted，与 P3-b 对称，建议做）
P3-b 已有 `has_ungrounded_identifier_terms`（:499）：query 有判别性 identifier 但**未**在证据中落地 → 硬拦。本项加它的对称放行：identifier **已**落地且 top chunk 有词法/identifier 命中 → 软 gate 放行。

1. 新增 `has_grounded_identifier_terms(analysis, top_group_items)`：复用现有 `strong_business_identifier_key`（:实现处）+ `compact_identifier_text` 抽取判别性 key，断言它出现在 **top_group** 的某条证据（content/heading/path）中。即 `has_ungrounded_identifier_terms` 的逻辑取反、但只在 top_group 内判定（避免被无关文档"借地落地"——与 P3-b 全局判定不同，这里要更严）。
2. 在 `decide_with_profile` 的放行汇总处（effective_score 计算 :316-322），把它并入释放条件：

```rust
let identifier_grounded_release = has_grounded_identifier_terms(analysis, &top_group_items)
    && has_any_chunk_lexical(top);   // top chunk 自身有词法/identifier 命中，防止仅靠同源他块落地
let effective_score = if (has_grounded_single_chunk_release
        || strong_semantic_context
        || identifier_grounded_release)
    && total_score < threshold
{
    threshold
} else {
    total_score
};
```

3. `refuse` / `reason` 同步加分支（:325-359）：`identifier_grounded_release` 为真时 reason 记 `"identifier_grounded_release"`，并让它满足 grounding（与 `strong_semantic_context` 并列计入 `has_grounding_signal || …`）。

> 安全性：不存在的代号（C090–C093）走 `has_ungrounded_identifier_terms` 在 §0-B 的硬拦里**先**被拦，永远到不了这条放行；外部/密钥/注入走 `intent_blocked` 硬拦。故 G2 不会放跑任何当前正确的拒答题。

### G3 — 可选：rank-1 落地 chunk 小额奖励（仅当 G1+G2 仍不够再加）
当 `top.lexical_strict_rank == Some(0)`（黄金 chunk 严格词法命中且排第一）时 `breakdown` 加一个小额项（+4~6），把"检索把对的 chunk 放到了首位"这一强信号纳入打分。G1+G2 大概率已够，留作备用，避免一次改太多难以归因。

## 2. 不要做
- **不降 Balanced 阈值**（55 不动）。
- **不碰** `has_ungrounded_identifier_terms` / `classify_query_intent` / 任何 P3 硬拦。
- **不碰** `engine_retrieve.rs` 召回链（召回已够，本轮纯打分）。

## 3. 单元测试（加在 `retrieval_eval.rs` 既有 `mod tests`）
1. `single_codename_full_coverage_passes_gate`：构造 support_terms 仅 1 个代号、coverage=1.0、document/lexical 中等的证据，断言 `!decision.refuse` 且 reason ∈ {`score_release`,`coverage_release`,`identifier_grounded_release`}。
2. `grounded_identifier_releases_when_score_just_below_threshold`：构造 total_score < 55 但 identifier 在 top_group 落地 + top chunk 有词法命中，断言放行且 reason == `identifier_grounded_release`。
3. `ungrounded_identifier_still_hard_blocked`（回归保护）：复用既有 NOVA-404 用例，断言仍 `entity_not_grounded`（确认 G2 没短路硬拦）。
4. `external_fact_low_coverage_still_refused`（回归保护）：构造无代号、coverage 低的外部事实证据，断言仍 `score_below_threshold` 拒答（确认 G1 没误放 C097 类）。

## 4. 验收（跑 #N+1：live · full_live · 100 题）
- `cargo fmt && cargo clippy -p memori-core -- -D warnings` → 0；`cargo test -p memori-core` 全绿。
- 跑回归 → `python scripts/export-retrieval-regression-excel.py` 更新台账。
- 期望 Δ（#2124 → #N+1）：
  - 回答题正确作答 **40/88 → ≥70/88**（救回 score_below_threshold 的 35 道里的多数）。
  - 拒答题正确拒答 **11/12 不掉**（C089–C100 路径不变）。
  - 整体 reject_correct **0.51 → ≥0.80**。
  - top3 文档召回不回退（≈0.61+）。
- **已知遗留（不在本轮 gate 范围）**：C094 一类"实体存在、但所问的具体属性在资料中不存在"的题，gate 无法仅凭检索信号与"属性存在"区分，应由**作答层 LLM 提示词**兜底（"若检索证据未直接覆盖所问属性，则明确说明资料未涉及并拒绝臆测"）。这是 answer-layer follow-up，单列，不要在 gate 里硬塞。
# 最新验收结果（2026-06-05，#N+5）

本文件下方保留的是 gating 方案形成过程和历史推理，旧数据不再代表当前最新指标。当前 source of truth 是：

- Report JSON: `target/retrieval-regression/live_embedding-full_live-1780648792/report.json`
- Mode/profile: `live_embedding + full_live`
- Corpus: `Memory_Test/` 100 cases

| 指标 | 起点 | 中途 | #N+5 最终 |
| --- | ---: | ---: | ---: |
| 整体 reject_correct | 0.56 | 0.74 | 0.91 |
| 回答题正确作答 | 45/88 | 63/88 | 80/88 |
| 拒答题正确拒答 | 11/12 | 11/12 | 11/12 |
| rerank 应用率 | 0.64 | 0.64 | 0.95 |
| top1 文档命中 | 0.375 | 0.59 | 0.875 |
| top3 文档召回 | 0.591 | 0.90 | 0.932 |
| top5 chunk 召回 | 0.69 | 0.92 | 0.932 |
| chunk_mrr | 0.55 | 0.83 | 0.856 |
| gating 误拒（已召回） | 35 | 20 | 6 |

验收裁决：

- 本轮 gating 目标已达成：整体 `reject_correct=0.91`，超过原目标 `>=0.80`。
- 回答题正确作答达到 `80/88`，超过原目标 `>=70/88`。
- 拒答题正确拒答保持 `11/12`，没有因 soft gate 放行出现整体安全回退。
- 剩余 `6` 道“文档已召回但仍被拒”的 case 应逐例处理，不建议继续粗暴降低 Balanced 阈值。
- `rerank_confident_release=35` 是当前主要放行路径，说明 reranker 原始置信度已经取代纯词法 coverage 成为更有效的答/拒信号。
