//! 50k 规模检索压测（审计 E7 / P2）——为 P1「单 `Mutex<Connection>` 串行化所有读写」
//! 的改造决策提供 P50/P95/P99 证据。
//!
//! 设计要点：
//! - 语料全程内存合成 + 确定性离线 embedding，不依赖 llama-server，CI 可复跑。
//! - 关闭 rerank（`MEMORI_RERANK_ENABLED=0`），隔离出**存储/检索**本身的延迟，
//!   正是 P1 单连接锁影响的部分。
//! - 两阶段测量：
//!   1. 顺序：单请求 P50/P95/P99 + 各阶段（doc_recall/doc_dense/chunk_lexical/
//!      chunk_dense/merge）耗时分布——回答「单查询在 50k 下多快」。
//!   2. 并发：固定并发度同时打，测每请求延迟与吞吐——若并发 P50 ≈ 并发度 × 顺序 P50，
//!      即证读被单连接串行化，P1 改造（WAL 只读连接池）有据可依。
//!
//! 用法：
//!   cargo run --release -p memori-core --example perf_scale -- \
//!     --docs 1000 --sections 50 --queries 300 --concurrency 8 --report target/perf_50k.json

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use memori_core::{
    MEMORI_DB_PATH_ENV, MemoriEngine, RetrievalMetrics, build_query_terms_for_offline_embedding,
};
use memori_parser::parse_and_chunk;
use serde::Serialize;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const EMBEDDING_DIM: usize = 256;

#[derive(Debug, Clone)]
struct Args {
    docs: usize,
    sections: usize,
    queries: usize,
    concurrency: usize,
    db_path: PathBuf,
    report_path: Option<PathBuf>,
}

fn parse_args() -> Result<Args, AnyError> {
    let cwd = std::env::current_dir()?;
    let mut docs = 1000usize;
    let mut sections = 50usize;
    let mut queries = 300usize;
    let mut concurrency = 8usize;
    let mut db_path = cwd.join("target").join("perf_scale.db");
    let mut report_path = None;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--docs" => docs = it.next().ok_or("--docs requires a value")?.parse()?,
            "--sections" => sections = it.next().ok_or("--sections requires a value")?.parse()?,
            "--queries" => queries = it.next().ok_or("--queries requires a value")?.parse()?,
            "--concurrency" => {
                concurrency = it.next().ok_or("--concurrency requires a value")?.parse()?
            }
            "--db-path" => {
                db_path = absolutize(&cwd, it.next().ok_or("--db-path requires a value")?)
            }
            "--report" => {
                report_path = Some(absolutize(
                    &cwd,
                    it.next().ok_or("--report requires a value")?,
                ))
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    Ok(Args {
        docs: docs.max(1),
        sections: sections.max(1),
        queries: queries.max(1),
        concurrency: concurrency.max(1),
        db_path,
        report_path,
    })
}

fn absolutize(cwd: &Path, value: String) -> PathBuf {
    let path = PathBuf::from(&value);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

#[derive(Debug, Clone, Serialize)]
struct LatencyStats {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    avg_ms: f64,
}

impl LatencyStats {
    fn from_micros(mut samples: Vec<u64>) -> Self {
        if samples.is_empty() {
            return Self {
                samples: 0,
                p50_ms: 0.0,
                p95_ms: 0.0,
                p99_ms: 0.0,
                max_ms: 0.0,
                avg_ms: 0.0,
            };
        }
        samples.sort_unstable();
        let n = samples.len();
        let avg = samples.iter().sum::<u64>() as f64 / n as f64;
        Self {
            samples: n,
            p50_ms: percentile(&samples, 0.50) / 1000.0,
            p95_ms: percentile(&samples, 0.95) / 1000.0,
            p99_ms: percentile(&samples, 0.99) / 1000.0,
            max_ms: samples[n - 1] as f64 / 1000.0,
            avg_ms: avg / 1000.0,
        }
    }
}

/// 最近秩法取分位（samples 已升序，单位微秒）。
fn percentile(sorted: &[u64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (q * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)] as f64
}

#[derive(Debug, Clone, Serialize)]
struct StageStats {
    doc_recall: LatencyStats,
    doc_dense: LatencyStats,
    chunk_lexical: LatencyStats,
    chunk_dense: LatencyStats,
    merge: LatencyStats,
}

#[derive(Debug, Clone, Serialize)]
struct PerfReport {
    app_version: String,
    docs: usize,
    sections_per_doc: usize,
    indexed_chunks: usize,
    queries: usize,
    concurrency: usize,
    index_ms: u64,
    sequential_total: LatencyStats,
    sequential_stages: StageStats,
    concurrent_total: LatencyStats,
    concurrent_wall_ms: u64,
    sequential_throughput_qps: f64,
    concurrent_throughput_qps: f64,
    /// 并发 P50 / 顺序 P50：≈1 表示并发无惩罚；≈并发度 表示读被完全串行化。
    contention_factor: f64,
    verdict: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), AnyError> {
    let args = parse_args()?;
    // 隔离存储/检索延迟：关闭 rerank，避免（若本机恰好在跑 :18004）外部往返污染测量。
    unsafe {
        std::env::set_var(MEMORI_DB_PATH_ENV, &args.db_path);
        std::env::set_var("MEMORI_RERANK_ENABLED", "0");
    }
    // 干净起点：删除旧 DB。
    let _ = std::fs::remove_file(&args.db_path);
    if let Some(parent) = args.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // watch root 须真实存在（bootstrap 校验），但合成文档不落盘——仅用作路径前缀。
    let synthetic_root = std::env::temp_dir().join("memori_perf_scale_corpus");
    std::fs::create_dir_all(&synthetic_root)?;
    let engine = MemoriEngine::bootstrap(synthetic_root.clone())?;

    eprintln!(
        "[perf] seeding {} docs x ~{} sections (offline deterministic embedding)...",
        args.docs, args.sections
    );
    let index_started = Instant::now();
    let indexed_chunks = seed_corpus(&engine, &synthetic_root, args.docs, args.sections).await?;
    let index_ms = index_started.elapsed().as_millis() as u64;
    eprintln!(
        "[perf] indexed {indexed_chunks} chunks in {index_ms} ms ({:.0} chunks/s)",
        indexed_chunks as f64 / (index_ms.max(1) as f64 / 1000.0)
    );

    // 查询定位到各文档的 needle，保证有真实检索工作量（非空召回）。
    let queries: Vec<String> = (0..args.queries)
        .map(|i| {
            let doc = i % args.docs;
            format!("needle{doc} 关于 主题 {} 的关键事实", doc % 97)
        })
        .collect();

    // 阶段一：顺序。
    eprintln!("[perf] sequential phase: {} queries...", queries.len());
    let engine = Arc::new(engine);
    let mut seq_total = Vec::with_capacity(queries.len());
    let mut doc_recall = Vec::with_capacity(queries.len());
    let mut doc_dense = Vec::with_capacity(queries.len());
    let mut chunk_lexical = Vec::with_capacity(queries.len());
    let mut chunk_dense = Vec::with_capacity(queries.len());
    let mut merge = Vec::with_capacity(queries.len());
    let seq_started = Instant::now();
    for q in &queries {
        let started = Instant::now();
        let metrics = run_query(&engine, q).await?;
        seq_total.push(started.elapsed().as_micros() as u64);
        doc_recall.push(metrics.doc_recall_ms * 1000);
        doc_dense.push(metrics.doc_dense_ms * 1000);
        chunk_lexical.push(metrics.chunk_lexical_ms * 1000);
        chunk_dense.push(metrics.chunk_dense_ms * 1000);
        merge.push(metrics.merge_ms * 1000);
    }
    let seq_wall_ms = seq_started.elapsed().as_millis() as u64;
    let sequential_total = LatencyStats::from_micros(seq_total);

    // 阶段二：并发。固定并发度，用有界并发分批跑。
    eprintln!(
        "[perf] concurrent phase: {} queries @ concurrency {}...",
        queries.len(),
        args.concurrency
    );
    let conc_started = Instant::now();
    let concurrent_samples = run_concurrent(&engine, &queries, args.concurrency).await?;
    let concurrent_wall_ms = conc_started.elapsed().as_millis() as u64;
    let concurrent_total = LatencyStats::from_micros(concurrent_samples);

    let seq_qps = queries.len() as f64 / (seq_wall_ms.max(1) as f64 / 1000.0);
    let conc_qps = queries.len() as f64 / (concurrent_wall_ms.max(1) as f64 / 1000.0);
    let contention_factor = if sequential_total.p50_ms > 0.0 {
        concurrent_total.p50_ms / sequential_total.p50_ms
    } else {
        0.0
    };
    let verdict = build_verdict(args.concurrency, contention_factor, conc_qps, seq_qps);

    let report = PerfReport {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        docs: args.docs,
        sections_per_doc: args.sections,
        indexed_chunks,
        queries: queries.len(),
        concurrency: args.concurrency,
        index_ms,
        sequential_total,
        sequential_stages: StageStats {
            doc_recall: LatencyStats::from_micros(doc_recall),
            doc_dense: LatencyStats::from_micros(doc_dense),
            chunk_lexical: LatencyStats::from_micros(chunk_lexical),
            chunk_dense: LatencyStats::from_micros(chunk_dense),
            merge: LatencyStats::from_micros(merge),
        },
        concurrent_total,
        concurrent_wall_ms,
        sequential_throughput_qps: seq_qps,
        concurrent_throughput_qps: conc_qps,
        contention_factor,
        verdict,
    };

    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = &args.report_path {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &json)?;
        eprintln!("[perf] report written to {}", path.display());
    }
    println!("{json}");
    print_human_summary(&report);
    Ok(())
}

async fn run_query(engine: &MemoriEngine, query: &str) -> Result<RetrievalMetrics, AnyError> {
    let embedding = build_deterministic_query_embedding(query, EMBEDDING_DIM);
    let inspection = engine
        .retrieve_structured_with_embedding(query, embedding, None, None)
        .await?;
    Ok(inspection.metrics)
}

/// 有界并发执行：维持至多 `concurrency` 个在飞请求，返回每请求延迟（微秒）。
async fn run_concurrent(
    engine: &Arc<MemoriEngine>,
    queries: &[String],
    concurrency: usize,
) -> Result<Vec<u64>, AnyError> {
    use tokio::sync::Semaphore;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(queries.len());
    for q in queries {
        let permit = semaphore.clone().acquire_owned().await?;
        let engine = Arc::clone(engine);
        let query = q.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            let started = Instant::now();
            let result = run_query(&engine, &query).await;
            result.map(|_| started.elapsed().as_micros() as u64)
        }));
    }
    let mut samples = Vec::with_capacity(handles.len());
    for handle in handles {
        samples.push(handle.await??);
    }
    Ok(samples)
}

/// 内存合成语料并写入索引；返回实际写入的 chunk 总数。
async fn seed_corpus(
    engine: &MemoriEngine,
    root: &Path,
    docs: usize,
    sections: usize,
) -> Result<usize, AnyError> {
    let state = engine.state();
    let store = state.vector_store.clone();
    store.begin_full_rebuild("perf_scale_seed").await?;
    store.purge_all_index_data().await?;

    let mut total_chunks = 0usize;
    for d in 0..docs {
        let path = root.join(format!("doc_{d:06}.md"));
        let text = synth_document(d, sections);
        let chunks = parse_and_chunk(&path, &text)?;
        if chunks.is_empty() {
            continue;
        }
        let embeddings = chunks
            .iter()
            .map(|chunk| build_deterministic_chunk_embedding(chunk, EMBEDDING_DIM))
            .collect::<Vec<_>>();
        let content_hash = stable_hash_hex(&text);
        store
            .replace_document_index(
                &path,
                Some(root),
                1_700_000_000 + d as i64,
                &content_hash,
                chunks.clone(),
                embeddings,
            )
            .await?;
        total_chunks += chunks.len();
        if d % 200 == 0 && d > 0 {
            eprintln!("[perf]   seeded {d}/{docs} docs ({total_chunks} chunks)...");
        }
    }

    store.finish_full_rebuild().await?;
    store.load_from_db().await?;
    Ok(total_chunks)
}

/// 合成一篇含 `sections` 节的 markdown；每节为独立小节（→ 大致一节一 chunk）。
/// doc d 的第 0 节埋入唯一 needle，使查询 `needle{d}` 可命中，制造真实检索工作量。
fn synth_document(doc: usize, sections: usize) -> String {
    let mut text = format!("# 文档 {doc}\n\n");
    for s in 0..sections {
        text.push_str(&format!("## 小节 {doc}-{s} 主题{}\n\n", (doc + s) % 97));
        if s == 0 {
            text.push_str(&format!(
                "needle{doc} 这里记录关于主题 {} 的关键事实与负责人安排。\n\n",
                doc % 97
            ));
        }
        // 从固定词表按 (doc,s,k) 取词，保证确定性且各 chunk 内容不同。
        let mut line = String::new();
        for k in 0..40 {
            let idx = (stable_hash_u64(&format!("{doc}-{s}-{k}")) as usize) % VOCAB.len();
            line.push_str(VOCAB[idx]);
            line.push(' ');
        }
        text.push_str(&line);
        text.push_str("\n\n");
    }
    text
}

const VOCAB: &[&str] = &[
    "项目",
    "进度",
    "风险",
    "预算",
    "里程碑",
    "负责人",
    "部门",
    "上线",
    "回滚",
    "审批",
    "指标",
    "延期",
    "依赖",
    "接口",
    "数据",
    "迁移",
    "测试",
    "发布",
    "版本",
    "缓存",
    "排期",
    "复盘",
    "故障",
    "容量",
    "限流",
    "鉴权",
    "审计",
    "合规",
    "成本",
    "优化",
    "schedule",
    "owner",
    "budget",
    "risk",
    "deadline",
    "release",
    "rollback",
    "metric",
    "incident",
    "capacity",
];

fn build_verdict(concurrency: usize, contention: f64, conc_qps: f64, seq_qps: f64) -> String {
    if concurrency <= 1 {
        return "并发度=1，未测争用；提高 --concurrency 以评估 P1 单连接串行化。".to_string();
    }
    let scaling = if seq_qps > 0.0 {
        conc_qps / seq_qps
    } else {
        0.0
    };
    if contention >= (concurrency as f64) * 0.7 {
        format!(
            "并发 P50 ≈ {contention:.1}× 顺序（并发度 {concurrency}），吞吐仅 {scaling:.2}× \
             ——读被单 Mutex<Connection> 严重串行化，P1 改造（WAL 只读连接池）收益明确。"
        )
    } else if contention >= 2.0 {
        format!(
            "并发 P50 ≈ {contention:.1}× 顺序，吞吐 {scaling:.2}×——存在部分串行化，P1 改造有中等收益。"
        )
    } else {
        format!(
            "并发 P50 ≈ {contention:.1}× 顺序，吞吐 {scaling:.2}×——当前规模下争用不显著，P1 可暂缓。"
        )
    }
}

fn print_human_summary(r: &PerfReport) {
    eprintln!("\n========== 50k 压测摘要 ==========");
    eprintln!(
        "语料: {} 文档 / {} chunks    建索引: {} ms",
        r.docs, r.indexed_chunks, r.index_ms
    );
    eprintln!(
        "顺序   P50/P95/P99: {:.1} / {:.1} / {:.1} ms   ({:.0} qps)",
        r.sequential_total.p50_ms,
        r.sequential_total.p95_ms,
        r.sequential_total.p99_ms,
        r.sequential_throughput_qps
    );
    eprintln!(
        "并发{:>2} P50/P95/P99: {:.1} / {:.1} / {:.1} ms   ({:.0} qps)",
        r.concurrency,
        r.concurrent_total.p50_ms,
        r.concurrent_total.p95_ms,
        r.concurrent_total.p99_ms,
        r.concurrent_throughput_qps
    );
    eprintln!(
        "阶段(顺序 P50): doc_recall {:.1} / doc_dense {:.1} / chunk_lex {:.1} / chunk_dense {:.1} / merge {:.1} ms",
        r.sequential_stages.doc_recall.p50_ms,
        r.sequential_stages.doc_dense.p50_ms,
        r.sequential_stages.chunk_lexical.p50_ms,
        r.sequential_stages.chunk_dense.p50_ms,
        r.sequential_stages.merge.p50_ms
    );
    eprintln!("争用系数: {:.2}", r.contention_factor);
    eprintln!("判定: {}", r.verdict);
    eprintln!("==================================");
}

// ---- 确定性 embedding 辅助（与 retrieval_regression.rs 同构）----

fn build_deterministic_chunk_embedding(chunk: &memori_core::DocumentChunk, dim: usize) -> Vec<f32> {
    let mut seed = String::new();
    if !chunk.heading_path.is_empty() {
        seed.push_str(&chunk.heading_path.join(" "));
        seed.push('\n');
    }
    seed.push_str(&chunk.file_path.to_string_lossy());
    seed.push('\n');
    seed.push_str(&chunk.content);
    build_deterministic_embedding(&build_query_terms_for_offline_embedding(&seed), dim)
}

fn build_deterministic_query_embedding(query: &str, dim: usize) -> Vec<f32> {
    build_deterministic_embedding(&build_query_terms_for_offline_embedding(query), dim)
}

fn build_deterministic_embedding(terms: &[String], dim: usize) -> Vec<f32> {
    let mut vector = vec![0.0_f32; dim];
    if dim == 0 {
        return vector;
    }
    let mut unique_terms = Vec::new();
    let mut seen = HashSet::new();
    for term in terms {
        let normalized = term.trim().to_ascii_lowercase();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            unique_terms.push(normalized);
        }
    }
    if unique_terms.is_empty() {
        return vector;
    }
    let weight = 1.0_f32 / (unique_terms.len() as f32).sqrt();
    for term in unique_terms {
        let h1 = stable_hash_u64(&(term.clone() + "#1"));
        let h2 = stable_hash_u64(&(term.clone() + "#2"));
        let i1 = (h1 as usize) % dim;
        let i2 = (h2 as usize) % dim;
        vector[i1] += weight;
        vector[i2] -= weight * 0.5;
    }
    vector
}

fn stable_hash_u64(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn stable_hash_hex(text: &str) -> String {
    format!("{:016x}", stable_hash_u64(text))
}
