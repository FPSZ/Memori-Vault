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

## 备注

- 顺序 P50 251 ms 中 `doc_recall` 185 ms 为大头——50k+ 规模下文档级召回（lexical+dense）是主成本，
  与 chunk 级相比更敏感于规模，后续若再扩规模应优先观察此项。
- 本测用确定性 embedding，**绝对延迟**反映存储/检索代码路径而非真实模型往返；用于纵向对比
  （改造前后、规模前后），不与 live 端到端答题延迟混淆。
