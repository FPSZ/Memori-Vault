# 50k 规模检索压测基线（审计 E7 / P2 / P1 证据）

> 工具：`memori-core/examples/perf_scale.rs`。内存合成语料 + 确定性离线 embedding，
> 关闭 rerank（隔离存储/检索延迟），不依赖 llama-server，可复跑。
> 复跑：`cargo run --release -p memori-core --example perf_scale -- --docs 1000 --sections 50 --queries 300 --concurrency 8 --report docs/qa/perf_50k_report.json`

## 本机基线（2026-06-13，1000 文档 / 101,000 chunks）

| 维度 | P50 | P95 | P99 | 吞吐 |
|---|---|---|---|---|
| 顺序（并发 1） | 251.4 ms | 330.0 ms | 365.1 ms | ~4 qps |
| 并发 8 | 1573.7 ms | 2193.5 ms | 2488.0 ms | ~5 qps |

- 建索引：96,944 ms（~1042 chunks/s，单写串行）。
- 顺序阶段分解（P50）：`doc_recall` 185 ms（主导）、`doc_dense` 30 ms、`chunk_lexical` 59 ms、`chunk_dense` 59 ms、`merge` 1 ms。

## 判定：P1（单 `Mutex<Connection>` 串行化所有读写）改造收益明确

**争用系数 = 并发 P50 / 顺序 P50 = 6.26×**（并发度 8）。即 8 路并发时单请求延迟劣化 6.3 倍，
而总吞吐仅从 4→5 qps（1.28×）。这正是单连接锁把并发读**串行化**的特征：增加并发几乎不增吞吐，
只堆积排队延迟。

→ P1 的「WAL 只读连接池 + 单写」改造（audit `store.rs:169` / `lib.rs:527`）有量化依据。
预期收益：读并发不再被写锁与彼此串行，并发 P50 应回落到接近顺序 P50，吞吐随并发近线性提升。
改造后用本 harness 同参复跑，对比争用系数应显著下降（目标 < 2×）。

## P1 改造结果（2026-06-13，WAL + 只读连接池）

改造内容：`store.rs` 写连接开 `journal_mode=WAL` + `busy_timeout=5s` + `synchronous=NORMAL`；
新增 N 路只读连接池（默认 4，`MEMORI_DB_READ_POOL_SIZE` 可调），检索热路径（search.rs
的 7 个 SELECT）从池取连接，WAL 下并发读互不阻塞、也不被写阻塞。同参复跑：

| 指标 | 改前（单 Mutex） | 改后（WAL+池） | 改善 |
|---|---|---|---|
| 顺序 P50 | 251.4 ms | 167.1 ms | −33% |
| 并发 8 P50 | 1573.7 ms | 286.6 ms | −82% |
| 并发 8 P95 | 2193.5 ms | 434.8 ms | −80% |
| **争用系数** | **6.26×** | **1.72×** | 达标（目标 < 2×） |
| 并发吞吐 | 4.9 qps | 26.7 qps | 5.4× |
| 建索引 | 96.9 s | 50.4 s | ~2×（WAL 写更快） |

→ 争用系数从 6.26× 降到 1.72×，并发吞吐提升 5.4×，顺序延迟也因 WAL + busy_timeout 降 33%，
建索引（单写）也快约 2×。P1 单连接串行化问题已解决；剩余 1.72× 主要是 doc_recall 阶段
（仍 119ms/P50）本身的计算成本与池上限（4 < 并发 8），如需进一步可调大池或优化文档级召回。

## 规模验证：10k / 50k（2026-09-07）

harness 同参复跑（确定性 embedding + 内存合成语料）：`--docs 10000` / `--docs 50000`
（50k = 50,000 docs × 50 sections = **5,050,000 chunks**），300 查询、并发 8。
报告：`target/perf_10k_validation.json` / `target/perf_50k_report.json`。

| 规模 | 顺序 P50 | 顺序 P95 | 并发 P50 | 并发 P95 | 争用系数 | 并发吞吐 |
|---|---|---|---|---|---|---|
| 1k（改后） | 167.1 ms | 179.5 ms | 286.6 ms | 434.8 ms | 1.72× | 26.7 qps |
| 10k | 1870.0 ms | 1904.6 ms | 3392.6 ms | 5502.0 ms | 1.81× | 2.20 qps |
| 50k | 9817.9 ms | 10020.7 ms | 18717.8 ms | 29792.1 ms | 1.91× | 0.41 qps（4.14× 顺序） |

- **争用系数**随规模亚线性上升（1.72 → 1.81 → 1.91×），保持在 <2× 目标内——P1 的
  WAL + 只读连接池结论在 50k 规模成立，单 `Mutex<Connection>` 不再是瓶颈顾虑。
- **绝对延迟成为新一轮关注点**：50k 顺序 P50 9.82s 中 `doc_recall` 占 7.97s（81%）；
  顺序 P99 18.1s / **max 40.4s**，且出现在无争用的顺序场景——长尾与争用无关；
  并发长尾 P99 36.0s / max 41.4s。规模 ×5（10k→50k）时 doc_recall 近似线性增长，
  文档级召回（lexical+dense 候选扫描）是唯一的规模敏感主项。
- **建索引**：10k `index_ms` 3,171,595 ms；50k `index_ms` 1,861,605 ms（含 50k 续跑时的
  FTS 重建开销；两次机器状态不同，数值不可直接对比）。
- 已知故障模式：50k 下偶发单查询 batch 延迟 40s+（对应 `chunk_lexical` max 23.9s），待观察。

## CI 复跑（2026-09-08 起）

workflow `.github/workflows/perf-scale.yml`（job `perf-scale`，ubuntu-latest）：

- **触发**：`workflow_dispatch` 手动（Actions 页面，可改 `docs`/`queries` 参数跑排查快档）+
  `schedule` 每周日 02:34 UTC 全量 50k；不挂在 PR 流程上（50k 一轮 ~2.5h）。
- **默认参数**：`--docs 50000 --sections 50 --queries 300 --concurrency 8`
- **断言**：`--max-contention-factor 2.0`——争用系数（并发 P50 / 顺序 P50）> 2.0 时
  进程非零退出、run 标红（与 1k/10k/50k 实测 1.72→1.81→1.91× 的留量一致）。
- **产物**：`perf-scale-report-<docs>` artifact 下的 `perf_scale_ci_report.json`（完整
  P50/P95/P99 与阶段分解），日志尾部有 PASS/FAIL 摘要行。

## 备注

- 顺序 P50 中 `doc_recall` 占比随规模上升（1k 74% → 50k 81%），且绝对值随规模近似线性
  增长——文档级召回（lexical+dense）是唯一规模敏感主项；若产品态目标为 50k+ 文档规模，
  优化应聚焦它（分阶段召回 / 文档级倒排截断）。
- 本测用确定性 embedding，**绝对延迟**反映存储/检索代码路径而非真实模型往返；用于纵向对比
  （改造前后、规模前后），不与 live 端到端答题延迟混淆。
