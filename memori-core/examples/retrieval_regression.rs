use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use memori_core::{
    AskStatus, MEMORI_DB_PATH_ENV, MemoriEngine, RetrievalInspection, RuntimeRetrievalBaseline,
    build_query_terms_for_offline_embedding,
};
use memori_parser::parse_and_chunk;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

const DEFAULT_OFFLINE_EMBEDDING_DIM: usize = 256;
const DEFAULT_MAX_INDEX_PREP_SECS: u64 = 180;
const DEFAULT_MAX_CASE_SECS: u64 = 30;
const DEFAULT_SUITE_PATH: &str = "docs/qa/retrieval_regression_suite.json";
const BASELINE_DOC_PATH: &str = "docs/qa/RETRIEVAL_BASELINE.md";

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
    target_clues: Vec<String>,
    document_hit_rank: Option<usize>,
    chunk_hit_rank: Option<usize>,
    top1_document_hit: bool,
    top3_document_recall: bool,
    top5_chunk_recall: bool,
    citation_valid: bool,
    reject_correct: bool,
    citations_count: usize,
    final_evidence_count: usize,
    doc_recall_ms: u64,
    chunk_lexical_ms: u64,
    chunk_dense_ms: u64,
    merge_ms: u64,
    notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionSummary {
    case_count: usize,
    answer_cases: usize,
    refuse_cases: usize,
    top1_document_hit_rate: f64,
    top3_document_recall_rate: f64,
    top5_chunk_recall_rate: f64,
    citation_validity_rate: f64,
    reject_correctness_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionReport {
    tool: &'static str,
    generated_at_utc: String,
    evaluation_mode: String,
    profile: String,
    suite_path: String,
    watch_root: String,
    db_path: String,
    baseline: RuntimeRetrievalBaseline,
    service_health: String,
    live_service_used: bool,
    index_prep_ms: Option<u64>,
    case_timeout_count: usize,
    preparation_error: Option<String>,
    summary: RegressionSummary,
    cases: Vec<RegressionCaseResult>,
}

#[derive(Debug, Clone)]
struct PreparationOutcome {
    service_health: ServiceHealth,
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
        generated_at_utc: unix_timestamp_string(),
        evaluation_mode: args.mode.as_str().to_string(),
        profile: args.profile.as_str().to_string(),
        suite_path: args.suite_path.to_string_lossy().to_string(),
        watch_root: args.watch_root.to_string_lossy().to_string(),
        db_path: args.db_path.to_string_lossy().to_string(),
        baseline,
        service_health: prep.service_health.as_str().to_string(),
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
    })
}

fn load_suite(
    path: &Path,
    case_filter: &Option<HashSet<String>>,
    profile: RegressionProfile,
) -> Result<RegressionSuite, AnyError> {
    let raw = fs::read_to_string(path)?;
    let raw = raw.trim_start_matches('\u{feff}');
    let mut suite: RegressionSuite = serde_json::from_str(&raw)?;
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
                live_service_used: false,
                index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
                preparation_error: None,
                run_cases: true,
            })
        }
        EvaluationMode::LiveEmbedding => prepare_live_engine(engine, args).await,
    }
}

async fn prepare_live_engine(
    engine: &MemoriEngine,
    args: &CliArgs,
) -> Result<PreparationOutcome, AnyError> {
    if !args.watch_root.exists() {
        return Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
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
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some("embedding provider returned an empty vector".to_string()),
                run_cases: false,
            });
        }
        Ok(Err(err)) => {
            return Ok(PreparationOutcome {
                service_health: ServiceHealth::Unavailable,
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some(format!("embedding provider probe failed: {err}")),
                run_cases: false,
            });
        }
        Err(_) => {
            return Ok(PreparationOutcome {
                service_health: ServiceHealth::Unavailable,
                live_service_used: true,
                index_prep_ms: None,
                preparation_error: Some("embedding provider probe timed out".to_string()),
                run_cases: false,
            });
        }
    }

    let started_at = Instant::now();
    match timeout(
        Duration::from_secs(args.max_index_prep_secs),
        engine.prepare_retrieval_index(),
    )
    .await
    {
        Ok(Ok(())) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Ready,
            live_service_used: true,
            index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
            preparation_error: None,
            run_cases: true,
        }),
        Ok(Err(err)) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Unavailable,
            live_service_used: true,
            index_prep_ms: Some(started_at.elapsed().as_millis() as u64),
            preparation_error: Some(format!("live index preparation failed: {err}")),
            run_cases: false,
        }),
        Err(_) => Ok(PreparationOutcome {
            service_health: ServiceHealth::Degraded,
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

    let documents = collect_offline_documents(suite);
    for relative_path in documents {
        let absolute_path = watch_root.join(&relative_path);
        if !absolute_path.exists() {
            return Err(format!(
                "offline regression target document does not exist: {}",
                absolute_path.display()
            )
            .into());
        }
        let raw_text = fs::read_to_string(&absolute_path)?;
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

fn collect_offline_documents(suite: &RegressionSuite) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut documents = Vec::new();
    for case in &suite.cases {
        if case.mode != RegressionMode::Answer {
            continue;
        }
        for document in &case.target_documents {
            let normalized = normalize_rel(document);
            if !normalized.is_empty() && seen.insert(normalized.clone()) {
                documents.push(normalized);
            }
        }
    }
    documents.sort();
    documents
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

    for case in &suite.cases {
        let outcome = match args.mode {
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
    let document_hit_rank = find_document_hit_rank(&inspection, &case.target_documents);
    let chunk_hit_rank = find_chunk_hit_rank(&inspection, &case.target_clues);
    let citation_valid = citations_are_valid(&inspection, watch_root, scope_paths);
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
        target_clues: case.target_clues.clone(),
        document_hit_rank,
        chunk_hit_rank,
        top1_document_hit: document_hit_rank == Some(1),
        top3_document_recall: document_hit_rank.is_some_and(|rank| rank <= 3),
        top5_chunk_recall: chunk_hit_rank.is_some_and(|rank| rank <= 5),
        citation_valid,
        reject_correct,
        citations_count: inspection.citations.len(),
        final_evidence_count: inspection.evidence.len(),
        doc_recall_ms: inspection.metrics.doc_recall_ms,
        chunk_lexical_ms: inspection.metrics.chunk_lexical_ms,
        chunk_dense_ms: inspection.metrics.chunk_dense_ms,
        merge_ms: inspection.metrics.merge_ms,
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
        target_clues: case.target_clues.clone(),
        document_hit_rank: None,
        chunk_hit_rank: None,
        top1_document_hit: false,
        top3_document_recall: false,
        top5_chunk_recall: false,
        citation_valid: false,
        reject_correct: false,
        citations_count: 0,
        final_evidence_count: 0,
        doc_recall_ms: 0,
        chunk_lexical_ms: 0,
        chunk_dense_ms: 0,
        merge_ms: 0,
        notes: case.notes.clone(),
    }
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
        citation_validity_rate: ratio(
            results.iter().filter(|case| case.citation_valid).count(),
            results.len(),
        ),
        reject_correctness_rate: ratio(
            results.iter().filter(|case| case.reject_correct).count(),
            results.len(),
        ),
    }
}

fn render_report_markdown(report: &RegressionReport) -> String {
    let mut out = String::new();
    out.push_str("# Retrieval Regression Report\n\n");
    out.push_str(&format!("Generated: `{}`\n\n", report.generated_at_utc));
    out.push_str("## Execution\n");
    out.push_str(&format!("- mode: `{}`\n", report.evaluation_mode));
    out.push_str(&format!("- profile: `{}`\n", report.profile));
    out.push_str(&format!("- watch_root: `{}`\n", report.watch_root));
    out.push_str(&format!("- db_path: `{}`\n", report.db_path));
    out.push_str(&format!("- service_health: `{}`\n", report.service_health));
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
        "- Top-5 chunk recall: {:.2}%\n",
        report.summary.top5_chunk_recall_rate * 100.0
    ));
    out.push_str(&format!(
        "- citation validity: {:.2}%\n",
        report.summary.citation_validity_rate * 100.0
    ));
    out.push_str(&format!(
        "- reject correctness: {:.2}%\n",
        report.summary.reject_correctness_rate * 100.0
    ));
    out.push_str(&format!(
        "- case timeouts: `{}`\n\n",
        report.case_timeout_count
    ));
    out.push_str("## Cases\n\n");
    out.push_str(
        "| ID | Status | Timed Out | Doc Rank | Chunk Rank | Citation Valid | Reject Correct |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for case in &report.cases {
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} |\n",
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
        ));
    }
    out
}

fn render_baseline_markdown(report: &RegressionReport) -> String {
    let offline = render_baseline_section(report, EvaluationMode::OfflineDeterministic);
    let live = render_baseline_section(report, EvaluationMode::LiveEmbedding);
    format!(
        "# Retrieval Baseline Snapshot\n\nUpdated: {} UTC\n\nThis document captures the current measured retrieval baseline for the rebuilt two-stage retrieval pipeline.\n\n## Offline Deterministic Baseline\n{}\n\n## Live Embedding Baseline\n{}\n\n## Report Source\n- suite: `{}`\n- generated report: `target/retrieval-regression/.../report.json`\n- runner: `cargo run -p memori-core --example retrieval_regression -- --suite {DEFAULT_SUITE_PATH} --watch-root .`\n",
        report.generated_at_utc, offline, live, report.suite_path
    )
}

fn render_baseline_section(report: &RegressionReport, mode: EvaluationMode) -> String {
    if report.evaluation_mode == mode.as_str() {
        format!(
            "- profile: `{}`\n- watch_root: `{}`\n- db_path: `{}`\n- embedding model: `{}`\n- embedding dim: `{}`\n- indexed documents: `{}`\n- indexed chunks: `{}`\n- rebuild_state: `{}`\n- service_health: `{}`\n- index_prep_ms: `{}`\n\n| Metric | Current Value | Notes |\n| --- | --- | --- |\n| `Top-1 document hit` | {:.2}% | answer cases only |\n| `Top-3 document recall` | {:.2}% | answer cases only |\n| `Top-5 chunk recall` | {:.2}% | answer cases only |\n| `citation validity` | {:.2}% | all cases |\n| `reject-correctness` | {:.2}% | all cases |\n| `index_prep_ms P50/P95` | N/A | single-run snapshot |\n",
            report.profile,
            report.watch_root,
            report.db_path,
            report.baseline.embedding_model_key,
            report.baseline.embedding_dim,
            report.baseline.indexed_document_count,
            report.baseline.indexed_chunk_count,
            report.baseline.rebuild_state,
            report.service_health,
            report
                .index_prep_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            report.summary.top1_document_hit_rate * 100.0,
            report.summary.top3_document_recall_rate * 100.0,
            report.summary.top5_chunk_recall_rate * 100.0,
            report.summary.citation_validity_rate * 100.0,
            report.summary.reject_correctness_rate * 100.0,
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
        DEFAULT_OFFLINE_EMBEDDING_DIM, EvaluationMode, RegressionCase, RegressionMode,
        RegressionProfile, build_deterministic_query_embedding,
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
}
