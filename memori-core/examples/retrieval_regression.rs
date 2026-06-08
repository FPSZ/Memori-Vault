use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use memori_core::{
    AskStatus, GatingBreakdown, MEMORI_DB_PATH_ENV, MemoriEngine, RetrievalInspection,
    RuntimeRetrievalBaseline, build_query_terms_for_offline_embedding,
};
use memori_parser::{extract_document_text, parse_and_chunk};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const DEFAULT_OFFLINE_EMBEDDING_DIM: usize = 256;
const DEFAULT_MAX_INDEX_PREP_SECS: u64 = 180;
const DEFAULT_MAX_CASE_SECS: u64 = 30;
const DEFAULT_SUITE_PATH: &str = "docs/qa/retrieval_regression_suite.json";
const BASELINE_DOC_PATH: &str = "docs/qa/RETRIEVAL_BASELINE.md";
const REGRESSION_REPORT_SCHEMA_VERSION: &str = "1.1";

#[derive(Debug, Clone)]
struct CliArgs {
    suite_path: PathBuf,
    watch_root: PathBuf,
    db_path: PathBuf,
    case_filter: Option<HashSet<String>>,
    write_baseline_doc: bool,
    mode: EvaluationMode,
    profile: RegressionProfile,
    max_index_prep_secs: u64,
    max_case_secs: u64,
    /// When set, the live seeder indexes every supported file under watch_root
    /// (haystack / distractor corpus), not just suite target documents.
    index_all: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum EvaluationMode {
    OfflineDeterministic,
    LiveEmbedding,
}

impl EvaluationMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::OfflineDeterministic => "offline_deterministic",
            Self::LiveEmbedding => "live_embedding",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "offline_deterministic" => Ok(Self::OfflineDeterministic),
            "live_embedding" => Ok(Self::LiveEmbedding),
            other => Err(format!("unsupported mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum RegressionProfile {
    CoreDocs,
    RepoMixed,
    FullLive,
}

impl RegressionProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::CoreDocs => "core_docs",
            Self::RepoMixed => "repo_mixed",
            Self::FullLive => "full_live",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "core_docs" => Ok(Self::CoreDocs),
            "repo_mixed" => Ok(Self::RepoMixed),
            "full_live" => Ok(Self::FullLive),
            other => Err(format!("unsupported profile: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ServiceHealth {
    Ready,
    Degraded,
    Unavailable,
}

impl ServiceHealth {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionSuite {
    version: u32,
    watch_root: String,
    notes: Option<String>,
    cases: Vec<RegressionCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionCase {
    id: String,
    query: String,
    mode: RegressionMode,
    #[serde(default)]
    scope_paths: Vec<String>,
    #[serde(default)]
    target_documents: Vec<String>,
    #[serde(default)]
    acceptable_documents: Vec<String>,
    #[serde(default)]
    target_clues: Vec<String>,
    #[serde(default)]
    profile_tags: Vec<String>,
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RegressionMode {
    Answer,
    Refuse,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionCaseResult {
    id: String,
    query: String,
    mode: RegressionMode,
    status: AskStatus,
    timed_out: bool,
    scope_paths: Vec<String>,
    target_documents: Vec<String>,
    acceptable_documents: Vec<String>,
    exact_document_hit_rank: Option<usize>,
    target_clues: Vec<String>,
    document_hit_rank: Option<usize>,
    chunk_hit_rank: Option<usize>,
    /// Deduplicated ranked document paths (citations first, then evidence),
    /// truncated to the first few — diagnostic for *which* doc beat the target.
    top_documents: Vec<String>,
    top1_document_hit: bool,
    top1_chunk_hit: bool,
    top3_document_recall: bool,
    top5_chunk_recall: bool,
    citation_valid: bool,
    reject_correct: bool,
    rerank_applied: bool,
    citations_count: usize,
    final_evidence_count: usize,
    gating_decision_reason: String,
    gating_score: i32,
    gating_breakdown: GatingBreakdown,
    top_rerank_raw_score: Option<f32>,
    doc_recall_ms: u64,
    doc_dense_ms: u64,
    chunk_lexical_ms: u64,
    chunk_dense_ms: u64,
    merge_ms: u64,
    rerank_ms: u64,
    /// Wall-clock latency for the whole retrieval+gating pass of this case.
    /// Note: the harness does not invoke the answer LLM, so this is
    /// retrieval/gating latency, not end-to-end answer-generation time.
    case_total_ms: u64,
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionSummary {
    case_count: usize,
    answer_cases: usize,
    refuse_cases: usize,
    top1_document_hit_rate: f64,
    top1_chunk_hit_rate: f64,
    top3_document_recall_rate: f64,
    top5_chunk_recall_rate: f64,
    chunk_mrr: f64,
    citation_validity_rate: f64,
    reject_correctness_rate: f64,
    rerank_applied_rate: f64,
    /// Retrieval+gating latency stats across non-timed-out cases (ms).
    avg_case_ms: f64,
    min_case_ms: u64,
    max_case_ms: u64,
    /// Record (not a scored metric): which cases were slowest / fastest.
    slowest_case_id: Option<String>,
    fastest_case_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionReport {
    tool: &'static str,
    report_schema_version: String,
    app_version: String,
    suite_version: u32,
    generated_at_utc: String,
    evaluation_mode: String,
    profile: String,
    suite_path: String,
    watch_root: String,
    db_path: String,
    baseline: RuntimeRetrievalBaseline,
    service_health: String,
    rerank_health: String,
    live_service_used: bool,
    index_prep_ms: Option<u64>,
    case_timeout_count: usize,
    preparation_error: Option<String>,
    summary: RegressionSummary,
    cases: Vec<RegressionCaseResult>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionProgress {
    run_id: String,
    status: String,
    total: usize,
    completed: usize,
    current_index: usize,
    current_case_id: String,
    current_query: String,
    current_mode: String,
    current_phase: String,
    passed: usize,
    failed: usize,
    updated_at_ms: u128,
}

#[derive(Debug, Clone)]
struct PreparationOutcome {
    service_health: ServiceHealth,
    rerank_health: String,
    live_service_used: bool,
    index_prep_ms: Option<u64>,
    preparation_error: Option<String>,
    run_cases: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), AnyError> {
    let args = parse_args()?;
    unsafe {
        std::env::set_var(MEMORI_DB_PATH_ENV, &args.db_path);
    }

    let suite = load_suite(&args.suite_path, &args.case_filter, args.profile)?;
    write_progress(
        &args.watch_root,
        RegressionProgress::preparing(suite.cases.len()),
    );
    let engine = MemoriEngine::bootstrap(args.watch_root.clone())?;
    let prep = prepare_engine(&engine, &args, &suite).await?;
    let baseline = engine.get_runtime_retrieval_baseline().await?;

    let (results, case_timeout_count) = if prep.run_cases {
        run_suite_cases(&engine, &args, &suite).await?
    } else {
        (Vec::new(), 0)
    };

    let report = RegressionReport {
        tool: "memori-core/examples/retrieval_regression.rs",
        report_schema_version: REGRESSION_REPORT_SCHEMA_VERSION.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        suite_version: suite.version,
        generated_at_utc: unix_timestamp_string(),
        evaluation_mode: args.mode.as_str().to_string(),
        profile: args.profile.as_str().to_string(),
        suite_path: args.suite_path.to_string_lossy().to_string(),
        watch_root: args.watch_root.to_string_lossy().to_string(),
        db_path: args.db_path.to_string_lossy().to_string(),
        baseline,
        service_health: prep.service_health.as_str().to_string(),
        rerank_health: prep.rerank_health,
        live_service_used: prep.live_service_used,
        index_prep_ms: prep.index_prep_ms,
        case_timeout_count,
        preparation_error: prep.preparation_error,
        summary: summarize(&results),
        cases: results,
    };

    let output_dir = args
        .watch_root
        .join("target")
        .join("retrieval-regression")
        .join(format!(
            "{}-{}-{}",
            args.mode.as_str(),
            args.profile.as_str(),
            timestamp_slug()
        ));
    fs::create_dir_all(&output_dir)?;
    fs::write(
        output_dir.join("report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    fs::write(
        output_dir.join("report.md"),
        render_report_markdown(&report),
    )?;
    let (passed, failed) = count_progress_outcomes(&report.cases);
    write_progress(
        &args.watch_root,
        RegressionProgress::done(suite.cases.len(), passed, failed),
    );

    if args.write_baseline_doc {
        fs::write(
            args.watch_root.join(BASELINE_DOC_PATH),
            render_baseline_markdown(&report),
        )?;
    }

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn parse_args() -> Result<CliArgs, AnyError> {
    let cwd = std::env::current_dir()?;
    let mut suite_path = cwd.join(DEFAULT_SUITE_PATH);
    let mut watch_root = cwd.clone();
    let mut db_path = None;
    let mut case_filter = None;
    let mut write_baseline_doc = false;
    let mut mode = EvaluationMode::OfflineDeterministic;
    let mut profile = RegressionProfile::CoreDocs;
    let mut max_index_prep_secs = DEFAULT_MAX_INDEX_PREP_SECS;
    let mut max_case_secs = DEFAULT_MAX_CASE_SECS;
    let mut index_all = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--suite" => {
                let value = args.next().ok_or("--suite requires a path")?;
                suite_path = absolutize(&cwd, value);
            }
            "--watch-root" => {
                let value = args.next().ok_or("--watch-root requires a path")?;
                watch_root = absolutize(&cwd, value);
            }
            "--db-path" => {
                let value = args.next().ok_or("--db-path requires a path")?;
                db_path = Some(absolutize(&cwd, value));
            }
            "--case" => {
                let value = args.next().ok_or("--case requires a value")?;
                case_filter = Some(
                    value
                        .split(',')
                        .map(|item| item.trim().to_string())
                        .filter(|item| !item.is_empty())
                        .collect::<HashSet<_>>(),
                );
            }
            "--write-baseline-doc" => write_baseline_doc = true,
            "--index-all" => index_all = true,
            "--mode" => {
                let value = args.next().ok_or("--mode requires a value")?;
                mode = EvaluationMode::parse(&value)?;
            }
            "--profile" => {
                let value = args.next().ok_or("--profile requires a value")?;
                profile = RegressionProfile::parse(&value)?;
            }
            "--max-index-prep-secs" => {
                let value = args
                    .next()
                    .ok_or("--max-index-prep-secs requires a value")?;
                max_index_prep_secs = value.parse()?;
            }
            "--max-case-secs" => {
                let value = args.next().ok_or("--max-case-secs requires a value")?;
                max_case_secs = value.parse()?;
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let db_path = db_path.unwrap_or_else(|| {
        watch_root
            .join("target")
            .join("retrieval-regression")
            .join(format!("{}-{}.db", mode.as_str(), profile.as_str()))
    });

    Ok(CliArgs {
        suite_path,
        watch_root,
        db_path,
        case_filter,
        write_baseline_doc,
        mode,
        profile,
        max_index_prep_secs,
        max_case_secs,
        index_all,
    })
}

fn load_suite(
    path: &Path,
    case_filter: &Option<HashSet<String>>,
    profile: RegressionProfile,
) -> Result<RegressionSuite, AnyError> {
    let raw = fs::read_to_string(path)?;
    let raw = raw.trim_start_matches('\u{feff}');
    let mut suite: RegressionSuite = serde_json::from_str(raw)?;
    suite.cases.retain(|case| {
        let matches_case_filter = case_filter
            .as_ref()
            .is_none_or(|filter| filter.contains(&case.id));
        let matches_profile = case.profile_tags.is_empty()
            || case
                .profile_tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(profile.as_str()));
        matches_case_filter && matches_profile
    });
    if suite.cases.is_empty() {
        return Err("regression suite contains no runnable cases".into());
    }
    Ok(suite)
}

async fn prepare_engine(
    engine: &MemoriEngine,
    args: &CliArgs,
    suite: &RegressionSuite,
) -> Result<PreparationOutcome, AnyError> {
    match args.mode {
        EvaluationMode::OfflineDeterministic => {
            let started_at = Instant::now();
            seed_offline_index(engine, &args.watch_root, suite).await?;
            Ok(PreparationOutcome {
                service_health: ServiceHealth::Ready,
                rerank_health: "disabled".to_string(),
                live_service_used: false,
                index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
                preparation_error: None,
                run_cases: true,
            })
        }
        EvaluationMode::LiveEmbedding => prepare_live_engine(engine, args, suite).await,
    }
}

async fn prepare_live_engine(
    engine: &MemoriEngine,
    args: &CliArgs,
    suite: &RegressionSuite,
) -> Result<PreparationOutcome, AnyError> {
    if !args.watch_root.exists() {
        return Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
            rerank_health: "unavailable".to_string(),
            live_service_used: true,
            index_prep_ms: None,
            preparation_error: Some(format!(
                "watch_root does not exist: {}",
                args.watch_root.display()
            )),
            run_cases: false,
        });
    }
    if !args.watch_root.is_dir() {
        return Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
            rerank_health: "unavailable".to_string(),
            live_service_used: true,
            index_prep_ms: None,
            preparation_error: Some(format!(
                "watch_root is not a directory: {}",
                args.watch_root.display()
            )),
            run_cases: false,
        });
    }

    if let Some(parent) = args.db_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&args.db_path)
        .is_err()
    {
        return Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
            rerank_health: "unavailable".to_string(),
            live_service_used: true,
            index_prep_ms: None,
            preparation_error: Some(format!(
                "db_path is not writable: {}",
                args.db_path.display()
            )),
            run_cases: false,
        });
    }

    let embed_health = timeout(
        Duration::from_secs(10),
        engine
            .state()
            .embedding_client
            .embed_text("memori regression health probe"),
    )
    .await;
    match embed_health {
        Ok(Ok(embedding)) if !embedding.is_empty() => {}
        Ok(Ok(_)) => {
            return Ok(PreparationOutcome {
                service_health: ServiceHealth::Unavailable,
                rerank_health: "unavailable".to_string(),
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some("embedding provider returned an empty vector".to_string()),
                run_cases: false,
            });
        }
        Ok(Err(err)) => {
            return Ok(PreparationOutcome {
                service_health: ServiceHealth::Unavailable,
                rerank_health: "unavailable".to_string(),
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some(format!("embedding provider probe failed: {err}")),
                run_cases: false,
            });
        }
        Err(_) => {
            return Ok(PreparationOutcome {
                service_health: ServiceHealth::Unavailable,
                rerank_health: "unavailable".to_string(),
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some("embedding provider probe timed out".to_string()),
                run_cases: false,
            });
        }
    }

    let rerank_health = if !engine.state().rerank_client.is_enabled() {
        "disabled".to_string()
    } else {
        match timeout(
            Duration::from_secs(5),
            engine
                .state()
                .rerank_client
                .rerank("probe", &["sample".to_string()]),
        )
        .await
        {
            Ok(Ok(_)) => "ready".to_string(),
            Ok(Err(_)) | Err(_) => "unavailable".to_string(),
        }
    };

    let started_at = Instant::now();
    match timeout(
        Duration::from_secs(args.max_index_prep_secs),
        seed_live_index(engine, &args.watch_root, suite, args.index_all),
    )
    .await
    {
        Ok(Ok(())) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Ready,
            rerank_health,
            live_service_used: true,
            index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
            preparation_error: None,
            run_cases: true,
        }),
        Ok(Err(err)) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
            rerank_health,
            live_service_used: true,
            index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
            preparation_error: Some(format!("live index preparation failed: {err}")),
            run_cases: false,
        }),
        Err(_) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Degraded,
            rerank_health,
            live_service_used: true,
            index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
            preparation_error: Some(format!(
                "live index preparation exceeded budget ({}s)",
                args.max_index_prep_secs
            )),
            run_cases: false,
        }),
    }
}

async fn seed_offline_index(
    engine: &MemoriEngine,
    watch_root: &Path,
    suite: &RegressionSuite,
) -> Result<(), AnyError> {
    let state = engine.state();
    let store = state.vector_store.clone();
    store
        .begin_full_rebuild("offline_deterministic_seed")
        .await?;
    store.purge_all_index_data().await?;

    let documents = collect_target_documents(suite);
    for relative_path in documents {
        let absolute_path = watch_root.join(&relative_path);
        if !absolute_path.exists() {
            return Err(format!(
                "offline regression target document does not exist: {}",
                absolute_path.display()
            )
            .into());
        }
        let raw_text = read_regression_document_text(&absolute_path)?;
        let chunks = parse_and_chunk(&absolute_path, &raw_text)?;
        if chunks.is_empty() {
            continue;
        }

        let embeddings = chunks
            .iter()
            .map(|chunk| build_deterministic_chunk_embedding(chunk, DEFAULT_OFFLINE_EMBEDDING_DIM))
            .collect::<Vec<_>>();
        let last_modified = file_modified_secs(&absolute_path)?;
        let content_hash = stable_hash_hex(&raw_text);

        store
            .replace_document_index(
                &absolute_path,
                Some(watch_root),
                last_modified,
                &content_hash,
                chunks,
                embeddings,
            )
            .await?;
    }

    store.finish_full_rebuild().await?;
    store.load_from_db().await?;
    Ok(())
}

async fn seed_live_index(
    engine: &MemoriEngine,
    watch_root: &Path,
    suite: &RegressionSuite,
    index_all: bool,
) -> Result<(), AnyError> {
    let state = engine.state();
    let store = state.vector_store.clone();
    store.begin_full_rebuild("live_regression_seed").await?;
    store.purge_all_index_data().await?;

    // Default: index only suite target/acceptable docs. With --index-all, index
    // every supported file under watch_root so distractor docs form a haystack;
    // suite target_documents still define what counts as a correct hit.
    let documents = if index_all {
        collect_supported_corpus_files(watch_root)?
    } else {
        collect_target_documents(suite)
    };
    for relative_path in documents {
        let absolute_path = watch_root.join(&relative_path);
        if !absolute_path.exists() {
            return Err(format!(
                "live regression target document does not exist: {}",
                absolute_path.display()
            )
            .into());
        }
        if !path_is_under_root(&absolute_path, watch_root) {
            return Err(format!(
                "live regression target document is outside watch_root: {}",
                absolute_path.display()
            )
            .into());
        }

        let raw_text = read_regression_document_text(&absolute_path)?;
        let chunks = parse_and_chunk(&absolute_path, &raw_text)?;
        if chunks.is_empty() {
            continue;
        }

        let prompts = chunks
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();
        let embeddings = state.embedding_client.embed_batch(&prompts).await?;
        if embeddings.len() != chunks.len() {
            return Err(format!(
                "embedding provider returned {} vectors for {} chunks in {}",
                embeddings.len(),
                chunks.len(),
                absolute_path.display()
            )
            .into());
        }

        let last_modified = file_modified_secs(&absolute_path)?;
        let content_hash = stable_hash_hex(&raw_text);
        store
            .replace_document_index(
                &absolute_path,
                Some(watch_root),
                last_modified,
                &content_hash,
                chunks,
                embeddings,
            )
            .await?;
    }

    store.finish_full_rebuild().await?;
    store.load_from_db().await?;
    Ok(())
}

fn collect_target_documents(suite: &RegressionSuite) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut documents = Vec::new();
    for case in &suite.cases {
        if case.mode != RegressionMode::Answer {
            continue;
        }
        for document in case
            .target_documents
            .iter()
            .chain(case.acceptable_documents.iter())
        {
            let normalized = normalize_rel(document);
            if !normalized.is_empty() && seen.insert(normalized.clone()) {
                documents.push(normalized);
            }
        }
    }
    documents.sort();
    documents
}

/// Walk `watch_root` recursively and return every supported corpus file as a
/// path relative to `watch_root`. Used by `--index-all` to index the full
/// haystack (signal + distractor docs), not just suite target documents.
fn collect_supported_corpus_files(watch_root: &Path) -> Result<Vec<String>, AnyError> {
    const EXTS: &[&str] = &[
        "md", "txt", "docx", "pdf", "pptx", "xlsx", "doc", "ppt", "xls",
    ];
    let mut out = Vec::new();
    let mut stack = vec![watch_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                // Skip build/output dirs: the harness writes its own report.md/json
                // under `<watch_root>/target/retrieval-regression/...`. Indexing those
                // would pollute the haystack with documents that quote every query
                // (incl. decoy codes), inflating grounding and defeating refuse gating.
                let dir_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default();
                if dir_name == "target" || dir_name.starts_with('.') {
                    continue;
                }
                stack.push(path);
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str())
                && EXTS.contains(&ext.to_ascii_lowercase().as_str())
                && let Ok(rel) = path.strip_prefix(watch_root)
            {
                let normalized = normalize_rel(&rel.to_string_lossy());
                if !normalized.is_empty() {
                    out.push(normalized);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn read_regression_document_text(path: &Path) -> Result<String, AnyError> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        ext.as_str(),
        "docx" | "pdf" | "pptx" | "xlsx" | "doc" | "ppt" | "xls"
    ) {
        extract_document_text(path)
            .ok_or_else(|| format!("failed to extract text from {}", path.display()).into())
    } else {
        Ok(fs::read_to_string(path)?)
    }
}

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

    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

async fn run_suite_cases(
    engine: &MemoriEngine,
    args: &CliArgs,
    suite: &RegressionSuite,
) -> Result<(Vec<RegressionCaseResult>, usize), AnyError> {
    let embedding_dim = match args.mode {
        EvaluationMode::OfflineDeterministic => DEFAULT_OFFLINE_EMBEDDING_DIM,
        EvaluationMode::LiveEmbedding => engine
            .get_runtime_retrieval_baseline()
            .await?
            .embedding_dim
            .max(DEFAULT_OFFLINE_EMBEDDING_DIM),
    };
    let mut timeouts = 0usize;
    let mut results = Vec::with_capacity(suite.cases.len());
    let mut passed = 0usize;
    let mut failed = 0usize;

    for (index, case) in suite.cases.iter().enumerate() {
        write_progress(
            &args.watch_root,
            RegressionProgress::running(suite.cases.len(), index, index + 1, case, passed, failed),
        );
        let case_started = Instant::now();
        let mut outcome = match args.mode {
            EvaluationMode::OfflineDeterministic => {
                run_case(
                    engine,
                    &args.watch_root,
                    case,
                    args.mode,
                    Some(build_deterministic_query_embedding(
                        &case.query,
                        embedding_dim,
                    )),
                )
                .await
            }
            EvaluationMode::LiveEmbedding => {
                match timeout(
                    Duration::from_secs(args.max_case_secs),
                    run_case(engine, &args.watch_root, case, args.mode, None),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        timeouts += 1;
                        Ok(timeout_case_result(case))
                    }
                }
            }
        }?;
        outcome.case_total_ms = case_started.elapsed().as_millis() as u64;
        if progress_case_passed(&outcome) {
            passed += 1;
        } else {
            failed += 1;
        }
        write_progress(
            &args.watch_root,
            RegressionProgress::running(
                suite.cases.len(),
                index + 1,
                index + 1,
                case,
                passed,
                failed,
            ),
        );
        results.push(outcome);
    }

    Ok((results, timeouts))
}

async fn run_case(
    engine: &MemoriEngine,
    watch_root: &Path,
    case: &RegressionCase,
    mode: EvaluationMode,
    injected_embedding: Option<Vec<f32>>,
) -> Result<RegressionCaseResult, AnyError> {
    let scope_paths = case
        .scope_paths
        .iter()
        .filter(|item| !item.trim().is_empty() && item.trim() != ".")
        .map(|item| watch_root.join(item))
        .collect::<Vec<_>>();
    let inspection = match (mode, injected_embedding) {
        (EvaluationMode::OfflineDeterministic, Some(query_embedding)) => {
            engine
                .retrieve_structured_with_embedding(
                    &case.query,
                    query_embedding,
                    if scope_paths.is_empty() {
                        None
                    } else {
                        Some(scope_paths.as_slice())
                    },
                    None,
                )
                .await?
        }
        _ => {
            engine
                .retrieve_structured(
                    &case.query,
                    if scope_paths.is_empty() {
                        None
                    } else {
                        Some(scope_paths.as_slice())
                    },
                    None,
                )
                .await?
        }
    };

    Ok(build_case_result(
        case,
        watch_root,
        &scope_paths,
        inspection,
        false,
    ))
}

fn build_case_result(
    case: &RegressionCase,
    watch_root: &Path,
    scope_paths: &[PathBuf],
    inspection: RetrievalInspection,
    timed_out: bool,
) -> RegressionCaseResult {
    let exact_document_hit_rank = find_document_hit_rank(&inspection, &case.target_documents);
    let document_hit_rank = find_document_hit_rank(&inspection, effective_document_targets(case));
    let chunk_hit_rank = find_chunk_hit_rank(&inspection, &case.target_clues);
    let citation_valid = citations_are_valid(&inspection, watch_root, scope_paths);
    let top_rerank_raw_score = inspection
        .evidence
        .first()
        .and_then(|item| item.rerank_raw_score);
    let reject_correct = matches!(
        (&case.mode, inspection.status),
        (RegressionMode::Refuse, AskStatus::InsufficientEvidence)
            | (RegressionMode::Answer, AskStatus::Answered)
            | (RegressionMode::Answer, AskStatus::ModelFailedWithEvidence)
    );

    RegressionCaseResult {
        id: case.id.clone(),
        query: case.query.clone(),
        mode: case.mode.clone(),
        status: inspection.status,
        timed_out,
        scope_paths: case.scope_paths.clone(),
        target_documents: case.target_documents.clone(),
        acceptable_documents: case.acceptable_documents.clone(),
        exact_document_hit_rank,
        target_clues: case.target_clues.clone(),
        document_hit_rank,
        chunk_hit_rank,
        top_documents: ranked_document_paths(&inspection)
            .into_iter()
            .take(5)
            .collect(),
        top1_document_hit: document_hit_rank == Some(1),
        top1_chunk_hit: chunk_hit_rank == Some(1),
        top3_document_recall: document_hit_rank.is_some_and(|rank| rank <= 3),
        top5_chunk_recall: chunk_hit_rank.is_some_and(|rank| rank <= 5),
        citation_valid,
        reject_correct,
        rerank_applied: inspection
            .metrics
            .query_flags
            .iter()
            .any(|flag| flag.contains("rerank:applied")),
        citations_count: inspection.citations.len(),
        final_evidence_count: inspection.evidence.len(),
        gating_decision_reason: inspection.metrics.gating_decision_reason.clone(),
        gating_score: inspection.metrics.gating_score,
        gating_breakdown: inspection.metrics.gating_breakdown.clone(),
        top_rerank_raw_score,
        doc_recall_ms: inspection.metrics.doc_recall_ms,
        doc_dense_ms: inspection.metrics.doc_dense_ms,
        chunk_lexical_ms: inspection.metrics.chunk_lexical_ms,
        chunk_dense_ms: inspection.metrics.chunk_dense_ms,
        merge_ms: inspection.metrics.merge_ms,
        rerank_ms: inspection.metrics.rerank_ms,
        case_total_ms: 0,
        notes: case.notes.clone(),
    }
}

fn timeout_case_result(case: &RegressionCase) -> RegressionCaseResult {
    RegressionCaseResult {
        id: case.id.clone(),
        query: case.query.clone(),
        mode: case.mode.clone(),
        status: AskStatus::InsufficientEvidence,
        timed_out: true,
        scope_paths: case.scope_paths.clone(),
        target_documents: case.target_documents.clone(),
        acceptable_documents: case.acceptable_documents.clone(),
        exact_document_hit_rank: None,
        target_clues: case.target_clues.clone(),
        document_hit_rank: None,
        chunk_hit_rank: None,
        top_documents: Vec::new(),
        top1_document_hit: false,
        top1_chunk_hit: false,
        top3_document_recall: false,
        top5_chunk_recall: false,
        citation_valid: false,
        reject_correct: false,
        rerank_applied: false,
        citations_count: 0,
        final_evidence_count: 0,
        gating_decision_reason: "timeout".to_string(),
        gating_score: 0,
        gating_breakdown: GatingBreakdown::default(),
        top_rerank_raw_score: None,
        doc_recall_ms: 0,
        doc_dense_ms: 0,
        chunk_lexical_ms: 0,
        chunk_dense_ms: 0,
        merge_ms: 0,
        rerank_ms: 0,
        case_total_ms: 0,
        notes: case.notes.clone(),
    }
}

fn effective_document_targets(case: &RegressionCase) -> &[String] {
    if case.acceptable_documents.is_empty() {
        &case.target_documents
    } else {
        &case.acceptable_documents
    }
}

/// Deduplicated ranked document paths (citations first, then any extra evidence
/// docs). Diagnostic only — used for the `top_documents` field to reveal *which*
/// doc beat the target. Does NOT drive `document_hit_rank` (kept byte-identical
/// to the original to avoid shifting the baseline metric).
fn ranked_document_paths(inspection: &RetrievalInspection) -> Vec<String> {
    let mut ranked_paths = Vec::new();
    for citation in &inspection.citations {
        let candidate = normalize_rel(&citation.relative_path);
        if !ranked_paths.contains(&candidate) {
            ranked_paths.push(candidate);
        }
    }
    for evidence in &inspection.evidence {
        let candidate = normalize_rel(&evidence.relative_path);
        if !ranked_paths.contains(&candidate) {
            ranked_paths.push(candidate);
        }
    }
    ranked_paths
}

fn find_document_hit_rank(
    inspection: &RetrievalInspection,
    target_documents: &[String],
) -> Option<usize> {
    if target_documents.is_empty() {
        return None;
    }

    let targets = target_documents
        .iter()
        .map(|item| normalize_rel(item))
        .collect::<HashSet<_>>();
    let mut ranked_paths = Vec::new();
    for citation in &inspection.citations {
        ranked_paths.push(normalize_rel(&citation.relative_path));
    }
    for evidence in &inspection.evidence {
        let candidate = normalize_rel(&evidence.relative_path);
        if !ranked_paths.contains(&candidate) {
            ranked_paths.push(candidate);
        }
    }

    ranked_paths
        .iter()
        .position(|item| targets.contains(item))
        .map(|idx| idx + 1)
}

fn find_chunk_hit_rank(inspection: &RetrievalInspection, target_clues: &[String]) -> Option<usize> {
    if target_clues.is_empty() {
        return None;
    }

    let normalized_clues = target_clues
        .iter()
        .map(|clue| clue.trim().to_lowercase())
        .filter(|clue| !clue.is_empty())
        .collect::<Vec<_>>();

    inspection
        .evidence
        .iter()
        .take(5)
        .position(|item| {
            let content = item.content.to_lowercase();
            normalized_clues.iter().any(|clue| content.contains(clue))
        })
        .map(|idx| idx + 1)
        .or_else(|| {
            inspection
                .citations
                .iter()
                .take(5)
                .position(|item| {
                    let excerpt = item.excerpt.to_lowercase();
                    normalized_clues.iter().any(|clue| excerpt.contains(clue))
                })
                .map(|idx| idx + 1)
        })
}

fn citations_are_valid(
    inspection: &RetrievalInspection,
    watch_root: &Path,
    scope_paths: &[PathBuf],
) -> bool {
    inspection.citations.iter().all(|citation| {
        let path = PathBuf::from(&citation.file_path);
        path.exists()
            && path_is_under_root(&path, watch_root)
            && (scope_paths.is_empty()
                || scope_paths
                    .iter()
                    .any(|scope| path_matches_scope(&path, scope)))
    })
}

fn path_is_under_root(path: &Path, root: &Path) -> bool {
    let normalized_path = normalize_abs(path);
    let normalized_root = normalize_abs(root);
    normalized_path == normalized_root || normalized_path.starts_with(&(normalized_root + "/"))
}

fn path_matches_scope(path: &Path, scope: &Path) -> bool {
    let normalized_path = normalize_abs(path);
    let normalized_scope = normalize_abs(scope);
    if scope.extension().is_some() {
        normalized_path == normalized_scope
    } else {
        normalized_path == normalized_scope
            || normalized_path.starts_with(&(normalized_scope + "/"))
    }
}

impl RegressionProgress {
    fn preparing(total: usize) -> Self {
        Self {
            run_id: String::new(),
            status: "running".to_string(),
            total,
            completed: 0,
            current_index: 0,
            current_case_id: String::new(),
            current_query: String::new(),
            current_mode: String::new(),
            current_phase: "preparing".to_string(),
            passed: 0,
            failed: 0,
            updated_at_ms: progress_now_ms(),
        }
    }

    fn running(
        total: usize,
        completed: usize,
        current_index: usize,
        case: &RegressionCase,
        passed: usize,
        failed: usize,
    ) -> Self {
        Self {
            run_id: String::new(),
            status: "running".to_string(),
            total,
            completed,
            current_index,
            current_case_id: case.id.clone(),
            current_query: case.query.clone(),
            current_mode: regression_mode_as_str(&case.mode).to_string(),
            current_phase: "running".to_string(),
            passed,
            failed,
            updated_at_ms: progress_now_ms(),
        }
    }

    fn done(total: usize, passed: usize, failed: usize) -> Self {
        Self {
            run_id: String::new(),
            status: "succeeded".to_string(),
            total,
            completed: total,
            current_index: total,
            current_case_id: String::new(),
            current_query: String::new(),
            current_mode: String::new(),
            current_phase: "done".to_string(),
            passed,
            failed,
            updated_at_ms: progress_now_ms(),
        }
    }
}

fn write_progress(watch_root: &Path, progress: RegressionProgress) {
    let progress_dir = watch_root.join("target").join("retrieval-regression");
    let progress_path = progress_dir.join(".active-progress.json");
    let _ = fs::create_dir_all(&progress_dir);
    if let Ok(raw) = serde_json::to_string_pretty(&progress) {
        let _ = fs::write(progress_path, raw);
    }
}

fn count_progress_outcomes(results: &[RegressionCaseResult]) -> (usize, usize) {
    let passed = results
        .iter()
        .filter(|case| progress_case_passed(case))
        .count();
    (passed, results.len().saturating_sub(passed))
}

fn progress_case_passed(case: &RegressionCaseResult) -> bool {
    if case.timed_out {
        return false;
    }
    match case.mode {
        RegressionMode::Answer => {
            case.top3_document_recall && case.top5_chunk_recall && case.citation_valid
        }
        RegressionMode::Refuse => case.reject_correct,
    }
}

fn regression_mode_as_str(mode: &RegressionMode) -> &'static str {
    match mode {
        RegressionMode::Answer => "answer",
        RegressionMode::Refuse => "refuse",
    }
}

fn progress_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn summarize(results: &[RegressionCaseResult]) -> RegressionSummary {
    let answer_cases = results
        .iter()
        .filter(|case| case.mode == RegressionMode::Answer)
        .count();
    let refuse_cases = results.len().saturating_sub(answer_cases);

    RegressionSummary {
        case_count: results.len(),
        answer_cases,
        refuse_cases,
        top1_document_hit_rate: ratio(
            results
                .iter()
                .filter(|case| case.mode == RegressionMode::Answer && case.top1_document_hit)
                .count(),
            answer_cases,
        ),
        top1_chunk_hit_rate: ratio(
            results
                .iter()
                .filter(|case| case.mode == RegressionMode::Answer && case.top1_chunk_hit)
                .count(),
            answer_cases,
        ),
        top3_document_recall_rate: ratio(
            results
                .iter()
                .filter(|case| case.mode == RegressionMode::Answer && case.top3_document_recall)
                .count(),
            answer_cases,
        ),
        top5_chunk_recall_rate: ratio(
            results
                .iter()
                .filter(|case| case.mode == RegressionMode::Answer && case.top5_chunk_recall)
                .count(),
            answer_cases,
        ),
        chunk_mrr: if answer_cases == 0 {
            0.0
        } else {
            results
                .iter()
                .filter(|case| case.mode == RegressionMode::Answer)
                .map(|case| {
                    case.chunk_hit_rank
                        .map(|rank| 1.0 / rank as f64)
                        .unwrap_or(0.0)
                })
                .sum::<f64>()
                / answer_cases as f64
        },
        citation_validity_rate: ratio(
            results.iter().filter(|case| case.citation_valid).count(),
            results.len(),
        ),
        reject_correctness_rate: ratio(
            results.iter().filter(|case| case.reject_correct).count(),
            results.len(),
        ),
        rerank_applied_rate: ratio(
            results.iter().filter(|case| case.rerank_applied).count(),
            results.len(),
        ),
        avg_case_ms: {
            let timed: Vec<u64> = results
                .iter()
                .filter(|case| !case.timed_out)
                .map(|case| case.case_total_ms)
                .collect();
            if timed.is_empty() {
                0.0
            } else {
                timed.iter().sum::<u64>() as f64 / timed.len() as f64
            }
        },
        min_case_ms: results
            .iter()
            .filter(|case| !case.timed_out)
            .map(|case| case.case_total_ms)
            .min()
            .unwrap_or(0),
        max_case_ms: results
            .iter()
            .filter(|case| !case.timed_out)
            .map(|case| case.case_total_ms)
            .max()
            .unwrap_or(0),
        slowest_case_id: results
            .iter()
            .filter(|case| !case.timed_out)
            .max_by_key(|case| case.case_total_ms)
            .map(|case| case.id.clone()),
        fastest_case_id: results
            .iter()
            .filter(|case| !case.timed_out)
            .min_by_key(|case| case.case_total_ms)
            .map(|case| case.id.clone()),
    }
}

fn render_report_markdown(report: &RegressionReport) -> String {
    let mut out = String::new();
    out.push_str("# Retrieval Regression Report\n\n");
    out.push_str(&format!("Generated: `{}`\n\n", report.generated_at_utc));
    out.push_str("## Execution\n");
    out.push_str(&format!(
        "- report_schema_version: `{}`\n",
        report.report_schema_version
    ));
    out.push_str(&format!("- app_version: `{}`\n", report.app_version));
    out.push_str(&format!("- suite_version: `{}`\n", report.suite_version));
    out.push_str(&format!("- mode: `{}`\n", report.evaluation_mode));
    out.push_str(&format!("- profile: `{}`\n", report.profile));
    out.push_str(&format!("- watch_root: `{}`\n", report.watch_root));
    out.push_str(&format!("- db_path: `{}`\n", report.db_path));
    out.push_str(&format!("- service_health: `{}`\n", report.service_health));
    out.push_str(&format!("- rerank_health: `{}`\n", report.rerank_health));
    out.push_str(&format!(
        "- live_service_used: `{}`\n",
        report.live_service_used
    ));
    out.push_str(&format!(
        "- index_prep_ms: `{}`\n",
        report
            .index_prep_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    ));
    if let Some(error) = &report.preparation_error {
        out.push_str(&format!("- preparation_error: `{}`\n", error));
    }
    out.push('\n');
    out.push_str("## Baseline\n");
    out.push_str(&format!(
        "- embedding: `{}` ({})\n",
        report.baseline.embedding_model_key, report.baseline.embedding_dim
    ));
    out.push_str(&format!(
        "- indexed: {} documents / {} chunks\n",
        report.baseline.indexed_document_count, report.baseline.indexed_chunk_count
    ));
    out.push_str(&format!(
        "- rebuild_state: `{}`\n\n",
        report.baseline.rebuild_state
    ));
    out.push_str("## Summary\n");
    out.push_str(&format!(
        "- Top-1 document hit: {:.2}%\n",
        report.summary.top1_document_hit_rate * 100.0
    ));
    out.push_str(&format!(
        "- Top-3 document recall: {:.2}%\n",
        report.summary.top3_document_recall_rate * 100.0
    ));
    out.push_str(&format!(
        "- Top-1 chunk hit: {:.2}%\n",
        report.summary.top1_chunk_hit_rate * 100.0
    ));
    out.push_str(&format!(
        "- Top-5 chunk recall: {:.2}%\n",
        report.summary.top5_chunk_recall_rate * 100.0
    ));
    out.push_str(&format!("- Chunk MRR: {:.4}\n", report.summary.chunk_mrr));
    out.push_str(&format!(
        "- citation validity: {:.2}%\n",
        report.summary.citation_validity_rate * 100.0
    ));
    out.push_str(&format!(
        "- reject correctness: {:.2}%\n",
        report.summary.reject_correctness_rate * 100.0
    ));
    out.push_str(&format!(
        "- rerank applied: {:.2}%\n",
        report.summary.rerank_applied_rate * 100.0
    ));
    out.push_str(&format!(
        "- case timeouts: `{}`\n\n",
        report.case_timeout_count
    ));
    out.push_str("## Cases\n\n");
    out.push_str(
        "| ID | Status | Timed Out | Doc Rank | Chunk Rank | Citation Valid | Reject Correct | Gating Reason |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for case in &report.cases {
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} | {} |\n",
            case.id,
            case.status,
            yes_no(case.timed_out),
            case.document_hit_rank
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            case.chunk_hit_rank
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            yes_no(case.citation_valid),
            yes_no(case.reject_correct),
            case.gating_decision_reason,
        ));
    }
    out
}

fn render_baseline_markdown(report: &RegressionReport) -> String {
    let offline = render_baseline_section(report, EvaluationMode::OfflineDeterministic);
    let live = render_baseline_section(report, EvaluationMode::LiveEmbedding);
    format!(
        "# Retrieval Baseline Snapshot\n\nUpdated: {} UTC\n\nThis document captures the current measured retrieval baseline for the rebuilt two-stage retrieval pipeline.\n\n## Offline Deterministic Baseline\n{}\n\n## Live Embedding Baseline\n{}\n\n## Report Source\n- suite: `{}`\n- suite_version: `{}`\n- app_version: `{}`\n- report_schema_version: `{}`\n- generated report: `target/retrieval-regression/.../report.json`\n- runner: `cargo run -p memori-core --example retrieval_regression -- --suite {DEFAULT_SUITE_PATH} --watch-root .`\n",
        report.generated_at_utc,
        offline,
        live,
        report.suite_path,
        report.suite_version,
        report.app_version,
        report.report_schema_version
    )
}

fn render_baseline_section(report: &RegressionReport, mode: EvaluationMode) -> String {
    if report.evaluation_mode == mode.as_str() {
        format!(
            "- profile: `{}`\n- watch_root: `{}`\n- db_path: `{}`\n- embedding model: `{}`\n- embedding dim: `{}`\n- indexed documents: `{}`\n- indexed chunks: `{}`\n- rebuild_state: `{}`\n- service_health: `{}`\n- rerank_health: `{}`\n- index_prep_ms: `{}`\n\n| Metric | Current Value | Notes |\n| --- | --- | --- |\n| `Top-1 document hit` | {:.2}% | answer cases only |\n| `Top-3 document recall` | {:.2}% | answer cases only |\n| `Top-1 chunk hit` | {:.2}% | answer cases only |\n| `Top-5 chunk recall` | {:.2}% | answer cases only |\n| `chunk MRR` | {:.4} | answer cases only |\n| `citation validity` | {:.2}% | all cases |\n| `reject-correctness` | {:.2}% | all cases |\n| `rerank applied` | {:.2}% | all cases |\n| `index_prep_ms P50/P95` | N/A | single-run snapshot |\n",
            report.profile,
            report.watch_root,
            report.db_path,
            report.baseline.embedding_model_key,
            report.baseline.embedding_dim,
            report.baseline.indexed_document_count,
            report.baseline.indexed_chunk_count,
            report.baseline.rebuild_state,
            report.service_health,
            report.rerank_health,
            report
                .index_prep_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            report.summary.top1_document_hit_rate * 100.0,
            report.summary.top3_document_recall_rate * 100.0,
            report.summary.top1_chunk_hit_rate * 100.0,
            report.summary.top5_chunk_recall_rate * 100.0,
            report.summary.chunk_mrr,
            report.summary.citation_validity_rate * 100.0,
            report.summary.reject_correctness_rate * 100.0,
            report.summary.rerank_applied_rate * 100.0,
        )
    } else {
        "- status: `N/A`\n- metrics: `N/A`\n".to_string()
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn normalize_rel(value: &str) -> String {
    value
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn normalize_abs(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        raw.to_ascii_lowercase().trim_end_matches('/').to_string()
    } else {
        raw.trim_end_matches('/').to_string()
    }
}

fn absolutize(base: &Path, value: String) -> PathBuf {
    let candidate = PathBuf::from(value);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn file_modified_secs(path: &Path) -> Result<i64, AnyError> {
    Ok(path
        .metadata()?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_secs() as i64)
}

fn stable_hash_hex(text: &str) -> String {
    format!("{:016x}", stable_hash_u64(text))
}

fn stable_hash_u64<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn timestamp_slug() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_OFFLINE_EMBEDDING_DIM, EvaluationMode, RegressionCase, RegressionCaseResult,
        RegressionMode, RegressionProfile, RegressionReport, RegressionSummary,
        build_deterministic_query_embedding, collect_target_documents, find_document_hit_rank,
        render_report_markdown, summarize,
    };
    use memori_core::{
        AnswerSourceMix, AskStatus, CitationItem, ContextBudgetReport, EvidenceItem, FailureClass,
        GatingBreakdown, RetrievalInspection, RetrievalMetrics, RuntimeRetrievalBaseline,
    };

    #[test]
    fn deterministic_query_embedding_is_stable() {
        let left = build_deterministic_query_embedding(
            "POST /api/ask week8_report.md",
            DEFAULT_OFFLINE_EMBEDDING_DIM,
        );
        let right = build_deterministic_query_embedding(
            "POST /api/ask week8_report.md",
            DEFAULT_OFFLINE_EMBEDDING_DIM,
        );
        assert_eq!(left, right);
    }

    #[test]
    fn profile_labels_are_stable() {
        assert_eq!(
            EvaluationMode::OfflineDeterministic.as_str(),
            "offline_deterministic"
        );
        assert_eq!(RegressionProfile::RepoMixed.as_str(), "repo_mixed");
    }

    #[test]
    fn case_profile_tags_can_target_core_docs() {
        let case = RegressionCase {
            id: "R".to_string(),
            query: "hello".to_string(),
            mode: RegressionMode::Answer,
            scope_paths: Vec::new(),
            target_documents: vec!["docs/planning/plan.md".to_string()],
            acceptable_documents: Vec::new(),
            target_clues: vec!["plan".to_string()],
            profile_tags: vec!["core_docs".to_string()],
            notes: None,
        };
        assert!(
            case.profile_tags
                .iter()
                .any(|tag| tag == RegressionProfile::CoreDocs.as_str())
        );
    }

    #[test]
    fn target_document_collection_uses_only_answer_targets() {
        let suite = super::RegressionSuite {
            version: 2,
            watch_root: ".".to_string(),
            notes: None,
            cases: vec![
                RegressionCase {
                    id: "C001".to_string(),
                    query: "q1".to_string(),
                    mode: RegressionMode::Answer,
                    scope_paths: Vec::new(),
                    target_documents: vec![
                        ".\\Memory_Test\\doc_001_产品策略_极光账本_制度.md".to_string(),
                        "Memory_Test/doc_002_产品策略_极光账本_会议纪要.txt".to_string(),
                    ],
                    acceptable_documents: vec!["Memory_Test/doc_003_topic_SOP.docx".to_string()],
                    target_clues: vec!["clue".to_string()],
                    profile_tags: vec!["core_docs".to_string()],
                    notes: None,
                },
                RegressionCase {
                    id: "C002".to_string(),
                    query: "q2".to_string(),
                    mode: RegressionMode::Answer,
                    scope_paths: Vec::new(),
                    target_documents: vec![
                        "Memory_Test/doc_001_产品策略_极光账本_制度.md".to_string(),
                    ],
                    acceptable_documents: Vec::new(),
                    target_clues: vec!["clue".to_string()],
                    profile_tags: vec!["core_docs".to_string()],
                    notes: None,
                },
                RegressionCase {
                    id: "C003".to_string(),
                    query: "q3".to_string(),
                    mode: RegressionMode::Refuse,
                    scope_paths: Vec::new(),
                    target_documents: vec!["README.md".to_string()],
                    acceptable_documents: Vec::new(),
                    target_clues: Vec::new(),
                    profile_tags: vec!["core_docs".to_string()],
                    notes: None,
                },
            ],
        };

        assert_eq!(
            collect_target_documents(&suite),
            vec![
                "Memory_Test/doc_001_产品策略_极光账本_制度.md".to_string(),
                "Memory_Test/doc_002_产品策略_极光账本_会议纪要.txt".to_string(),
                "Memory_Test/doc_003_topic_SOP.docx".to_string(),
            ]
        );
    }

    #[test]
    fn acceptable_documents_can_match_sibling_fact_card_document() {
        let inspection = RetrievalInspection {
            status: AskStatus::Answered,
            question: "q".to_string(),
            scope_paths: Vec::new(),
            citations: vec![CitationItem {
                index: 1,
                file_path: "D:/repo/Memory_Test/doc_002_topic_meeting.txt".to_string(),
                relative_path: "Memory_Test/doc_002_topic_meeting.txt".to_string(),
                chunk_index: 0,
                heading_path: Vec::new(),
                excerpt: "shared fact".to_string(),
            }],
            evidence: vec![EvidenceItem {
                file_path: "D:/repo/Memory_Test/doc_002_topic_meeting.txt".to_string(),
                relative_path: "Memory_Test/doc_002_topic_meeting.txt".to_string(),
                chunk_index: 0,
                heading_path: Vec::new(),
                block_kind: "paragraph".to_string(),
                document_reason: "lexical_strict".to_string(),
                reason: "lexical_strict".to_string(),
                document_rank: 1,
                chunk_rank: 1,
                document_raw_score: Some(1.0),
                lexical_raw_score: Some(1.0),
                dense_raw_score: None,
                rerank_raw_score: None,
                final_score: 1.0,
                content: "shared fact".to_string(),
            }],
            metrics: RetrievalMetrics::default(),
            answer_source_mix: AnswerSourceMix::DocumentOnly,
            memory_context: Vec::new(),
            source_groups: Vec::new(),
            failure_class: FailureClass::None,
            context_budget_report: ContextBudgetReport::default(),
        };

        let exact_rank = find_document_hit_rank(
            &inspection,
            &["Memory_Test/doc_001_topic_policy.md".to_string()],
        );
        let acceptable_rank = find_document_hit_rank(
            &inspection,
            &[
                "Memory_Test/doc_001_topic_policy.md".to_string(),
                "Memory_Test/doc_002_topic_meeting.txt".to_string(),
            ],
        );

        assert_eq!(exact_rank, None);
        assert_eq!(acceptable_rank, Some(1));
    }

    #[test]
    fn summarize_reports_chunk_and_rerank_metrics() {
        let results = vec![
            RegressionCaseResult {
                id: "R1".to_string(),
                query: "q1".to_string(),
                mode: RegressionMode::Answer,
                status: AskStatus::Answered,
                timed_out: false,
                scope_paths: Vec::new(),
                target_documents: vec!["README.md".to_string()],
                acceptable_documents: Vec::new(),
                exact_document_hit_rank: Some(1),
                target_clues: vec!["desktop-first".to_string()],
                document_hit_rank: Some(1),
                chunk_hit_rank: Some(1),
                top1_document_hit: true,
                top1_chunk_hit: true,
                top3_document_recall: true,
                top5_chunk_recall: true,
                citation_valid: true,
                reject_correct: true,
                rerank_applied: true,
                citations_count: 1,
                final_evidence_count: 1,
                gating_decision_reason: "semantic_context_release".to_string(),
                gating_score: 55,
                gating_breakdown: GatingBreakdown::default(),
                top_rerank_raw_score: Some(0.42),
                doc_recall_ms: 10,
                doc_dense_ms: 4,
                chunk_lexical_ms: 2,
                chunk_dense_ms: 3,
                merge_ms: 1,
                rerank_ms: 6,
                case_total_ms: 12,
                notes: None,
            },
            RegressionCaseResult {
                id: "R2".to_string(),
                query: "q2".to_string(),
                mode: RegressionMode::Answer,
                status: AskStatus::Answered,
                timed_out: false,
                scope_paths: Vec::new(),
                target_documents: vec!["docs/guides/TUTORIAL.md".to_string()],
                acceptable_documents: Vec::new(),
                exact_document_hit_rank: Some(2),
                target_clues: vec!["low".to_string()],
                document_hit_rank: Some(2),
                chunk_hit_rank: Some(4),
                top1_document_hit: false,
                top1_chunk_hit: false,
                top3_document_recall: true,
                top5_chunk_recall: true,
                citation_valid: true,
                reject_correct: true,
                rerank_applied: false,
                citations_count: 1,
                final_evidence_count: 1,
                gating_decision_reason: "lexical_context_release".to_string(),
                gating_score: 55,
                gating_breakdown: GatingBreakdown::default(),
                top_rerank_raw_score: None,
                doc_recall_ms: 11,
                doc_dense_ms: 5,
                chunk_lexical_ms: 2,
                chunk_dense_ms: 4,
                merge_ms: 1,
                rerank_ms: 0,
                case_total_ms: 5,
                notes: None,
            },
            RegressionCaseResult {
                id: "R3".to_string(),
                query: "q3".to_string(),
                mode: RegressionMode::Refuse,
                status: AskStatus::InsufficientEvidence,
                timed_out: false,
                scope_paths: Vec::new(),
                target_documents: Vec::new(),
                acceptable_documents: Vec::new(),
                exact_document_hit_rank: None,
                target_clues: Vec::new(),
                document_hit_rank: None,
                chunk_hit_rank: None,
                top1_document_hit: false,
                top1_chunk_hit: false,
                top3_document_recall: false,
                top5_chunk_recall: false,
                citation_valid: true,
                reject_correct: true,
                rerank_applied: true,
                citations_count: 0,
                final_evidence_count: 0,
                gating_decision_reason: "insufficient_evidence".to_string(),
                gating_score: 0,
                gating_breakdown: GatingBreakdown::default(),
                top_rerank_raw_score: None,
                doc_recall_ms: 0,
                doc_dense_ms: 0,
                chunk_lexical_ms: 0,
                chunk_dense_ms: 0,
                merge_ms: 0,
                rerank_ms: 0,
                case_total_ms: 5,
                notes: None,
            },
        ];

        let summary = summarize(&results);
        assert_eq!(summary.case_count, 3);
        assert_eq!(summary.answer_cases, 2);
        assert_eq!(summary.refuse_cases, 1);
        assert!((summary.top1_document_hit_rate - 0.5).abs() < f64::EPSILON);
        assert!((summary.top1_chunk_hit_rate - 0.5).abs() < f64::EPSILON);
        assert!((summary.top3_document_recall_rate - 1.0).abs() < f64::EPSILON);
        assert!((summary.top5_chunk_recall_rate - 1.0).abs() < f64::EPSILON);
        assert!((summary.chunk_mrr - 0.625).abs() < 1e-9);
        assert!((summary.citation_validity_rate - 1.0).abs() < f64::EPSILON);
        assert!((summary.reject_correctness_rate - 1.0).abs() < f64::EPSILON);
        assert!((summary.rerank_applied_rate - (2.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn markdown_report_includes_rerank_health_and_gating_reason() {
        let report = RegressionReport {
            tool: "memori-core/examples/retrieval_regression.rs",
            report_schema_version: "1.1".to_string(),
            app_version: "0.1.0".to_string(),
            suite_version: 2,
            generated_at_utc: "1".to_string(),
            evaluation_mode: "live_embedding".to_string(),
            profile: "full_live".to_string(),
            suite_path: "docs/qa/retrieval_regression_suite.json".to_string(),
            watch_root: ".".to_string(),
            db_path: "target/retrieval-regression/live.db".to_string(),
            baseline: RuntimeRetrievalBaseline {
                watch_root: Some(".".to_string()),
                resolved_db_path: "target/retrieval-regression/live.db".to_string(),
                embedding_model_key: "test-embed".to_string(),
                embedding_dim: 256,
                indexed_document_count: 3,
                indexed_chunk_count: 7,
                rebuild_state: "ready".to_string(),
            },
            service_health: "ready".to_string(),
            rerank_health: "ready".to_string(),
            live_service_used: true,
            index_prep_ms: Some(42),
            case_timeout_count: 0,
            preparation_error: None,
            summary: RegressionSummary {
                case_count: 1,
                answer_cases: 1,
                refuse_cases: 0,
                top1_document_hit_rate: 1.0,
                top1_chunk_hit_rate: 1.0,
                top3_document_recall_rate: 1.0,
                top5_chunk_recall_rate: 1.0,
                chunk_mrr: 1.0,
                citation_validity_rate: 1.0,
                reject_correctness_rate: 1.0,
                rerank_applied_rate: 1.0,
                avg_case_ms: 12.0,
                min_case_ms: 12,
                max_case_ms: 12,
                slowest_case_id: Some("R1".to_string()),
                fastest_case_id: Some("R1".to_string()),
            },
            cases: vec![RegressionCaseResult {
                id: "R1".to_string(),
                query: "q1".to_string(),
                mode: RegressionMode::Answer,
                status: AskStatus::Answered,
                timed_out: false,
                scope_paths: Vec::new(),
                target_documents: vec!["README.md".to_string()],
                acceptable_documents: Vec::new(),
                exact_document_hit_rank: Some(1),
                target_clues: vec!["desktop-first".to_string()],
                document_hit_rank: Some(1),
                chunk_hit_rank: Some(1),
                top1_document_hit: true,
                top1_chunk_hit: true,
                top3_document_recall: true,
                top5_chunk_recall: true,
                citation_valid: true,
                reject_correct: true,
                rerank_applied: true,
                citations_count: 1,
                final_evidence_count: 1,
                gating_decision_reason: "semantic_context_release".to_string(),
                gating_score: 55,
                gating_breakdown: GatingBreakdown::default(),
                top_rerank_raw_score: Some(0.42),
                doc_recall_ms: 10,
                doc_dense_ms: 4,
                chunk_lexical_ms: 2,
                chunk_dense_ms: 3,
                merge_ms: 1,
                rerank_ms: 6,
                case_total_ms: 12,
                notes: None,
            }],
        };

        let markdown = render_report_markdown(&report);
        assert!(markdown.contains("- report_schema_version: `1.1`"));
        assert!(markdown.contains("- app_version: `0.1.0`"));
        assert!(markdown.contains("- suite_version: `2`"));
        assert!(markdown.contains("- rerank_health: `ready`"));
        assert!(markdown.contains("- rerank applied: 100.00%"));
        assert!(markdown.contains("- Chunk MRR: 1.0000"));
        assert!(markdown.contains("| ID | Status | Timed Out | Doc Rank | Chunk Rank | Citation Valid | Reject Correct | Gating Reason |"));
        assert!(markdown.contains("semantic_context_release"));
    }
}
