use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use memori_core::{
    AskResponseStructured, AskStatus, MEMORI_DB_PATH_ENV, MemoriEngine, RuntimeRetrievalBaseline,
};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

const DEFAULT_MAX_INDEX_PREP_SECS: u64 = 180;
const DEFAULT_MAX_CASE_SECS: u64 = 30;
const DEFAULT_TOP_K: usize = 10;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
struct CliArgs {
    corpus_root: PathBuf,
    questions_path: PathBuf,
    db_path: PathBuf,
    lang: Option<String>,
    top_k: usize,
    max_index_prep_secs: u64,
    max_case_secs: u64,
    report_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsabilityQuestionFile {
    suite_name: Option<String>,
    notes: Option<String>,
    questions: Vec<UsabilityQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsabilityQuestion {
    id: String,
    question: String,
    #[serde(default = "default_question_mode")]
    mode: QuestionMode,
    #[serde(default)]
    scope_paths: Vec<String>,
    #[serde(default)]
    expected_citation_suffixes: Vec<String>,
    #[serde(default)]
    expected_answer_contains: Vec<String>,
    #[serde(default)]
    forbidden_answer_contains: Vec<String>,
    notes: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum QuestionMode {
    Answer,
    Refuse,
}

fn default_question_mode() -> QuestionMode {
    QuestionMode::Answer
}

#[derive(Debug, Clone, Serialize)]
struct UsabilityReport {
    tool: &'static str,
    generated_at_utc: String,
    suite_name: Option<String>,
    corpus_root: String,
    questions_path: String,
    db_path: String,
    baseline: RuntimeRetrievalBaseline,
    corpus_document_count: usize,
    summary: UsabilitySummary,
    results: Vec<UsabilityQuestionResult>,
}

#[derive(Debug, Clone, Serialize)]
struct UsabilitySummary {
    question_count: usize,
    answer_questions: usize,
    refuse_questions: usize,
    usable_answer_count: usize,
    pass_count: usize,
    fail_count: usize,
    pass_rate: f64,
    gate_passed: bool,
    corpus_target_met: bool,
    no_false_answered_refusal: bool,
}

#[derive(Debug, Clone, Serialize)]
struct UsabilityQuestionResult {
    id: String,
    question: String,
    mode: QuestionMode,
    status: AskStatus,
    pass: bool,
    failure_class: Option<FailureClass>,
    reasons: Vec<String>,
    answer: String,
    citations: Vec<String>,
    citation_valid: bool,
    evidence_count: usize,
    notes: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureClass {
    RetrievalMiss,
    GatingFalseRefusal,
    AnswerSynthesisFail,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), AnyError> {
    let args = parse_args()?;
    fs::create_dir_all(&args.report_dir)?;
    unsafe {
        std::env::set_var(MEMORI_DB_PATH_ENV, &args.db_path);
    }

    let question_file = load_question_file(&args.questions_path)?;
    let engine = MemoriEngine::bootstrap(args.corpus_root.clone())?;
    prepare_engine(&engine, args.max_index_prep_secs).await?;
    let baseline = engine.get_runtime_retrieval_baseline().await?;
    let corpus_document_count = count_supported_documents(&args.corpus_root)?;
    let results = run_questions(&engine, &args, &question_file).await?;
    let summary = summarize_results(&results, corpus_document_count);

    let report = UsabilityReport {
        tool: "memori-core/examples/usability_smoke.rs",
        generated_at_utc: unix_timestamp_string(),
        suite_name: question_file.suite_name,
        corpus_root: args.corpus_root.to_string_lossy().to_string(),
        questions_path: args.questions_path.to_string_lossy().to_string(),
        db_path: args.db_path.to_string_lossy().to_string(),
        baseline,
        corpus_document_count,
        summary,
        results,
    };

    let json_path = args.report_dir.join("report.json");
    let md_path = args.report_dir.join("report.md");
    fs::write(&json_path, serde_json::to_vec_pretty(&report)?)?;
    fs::write(&md_path, render_markdown_report(&report))?;

    println!("========================================");
    println!("Usability smoke finished");
    println!("JSON report: {}", json_path.display());
    println!("Markdown report: {}", md_path.display());
    println!("Usable answers: {}/{}", report.summary.usable_answer_count, report.summary.question_count);
    println!("Gate passed: {}", yes_no(report.summary.gate_passed));
    println!("========================================");

    Ok(())
}

fn parse_args() -> Result<CliArgs, AnyError> {
    let mut corpus_root = None;
    let mut questions_path = None;
    let mut db_path = None;
    let mut lang = Some("zh-CN".to_string());
    let mut top_k = DEFAULT_TOP_K;
    let mut max_index_prep_secs = DEFAULT_MAX_INDEX_PREP_SECS;
    let mut max_case_secs = DEFAULT_MAX_CASE_SECS;
    let mut report_dir = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--corpus-root" => corpus_root = args.next().map(PathBuf::from),
            "--questions" => questions_path = args.next().map(PathBuf::from),
            "--db-path" => db_path = args.next().map(PathBuf::from),
            "--lang" => lang = args.next(),
            "--top-k" => {
                top_k = args
                    .next()
                    .ok_or("missing value for --top-k")?
                    .parse::<usize>()?;
            }
            "--max-index-prep-secs" => {
                max_index_prep_secs = args
                    .next()
                    .ok_or("missing value for --max-index-prep-secs")?
                    .parse::<u64>()?;
            }
            "--max-case-secs" => {
                max_case_secs = args
                    .next()
                    .ok_or("missing value for --max-case-secs")?
                    .parse::<u64>()?;
            }
            "--report-dir" => report_dir = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unsupported arg: {other}").into()),
        }
    }

    let corpus_root = corpus_root.ok_or("missing required --corpus-root")?;
    let questions_path = questions_path.unwrap_or_else(|| corpus_root.join("memori-usability-questions.json"));
    let stamp = unix_timestamp_secs();
    let db_path = db_path.unwrap_or_else(|| {
        PathBuf::from("target")
            .join("usability-smoke")
            .join(format!("smoke-{stamp}.db"))
    });
    let report_dir = report_dir.unwrap_or_else(|| {
        PathBuf::from("target")
            .join("usability-smoke")
            .join(format!("run-{stamp}"))
    });

    Ok(CliArgs {
        corpus_root,
        questions_path,
        db_path,
        lang,
        top_k,
        max_index_prep_secs,
        max_case_secs,
        report_dir,
    })
}

fn print_help() {
    println!("memori-core usability smoke");
    println!("  --corpus-root <path>         External corpus directory (required)");
    println!("  --questions <path>           Question JSON file (default: <corpus-root>/memori-usability-questions.json)");
    println!("  --db-path <path>             SQLite db path for this smoke run");
    println!("  --report-dir <path>          Output directory for report.json/report.md");
    println!("  --lang <lang>                Answer language (default: zh-CN)");
    println!("  --top-k <n>                  Final answer top-k (default: 10)");
    println!("  --max-index-prep-secs <n>    Index preparation timeout (default: 180)");
    println!("  --max-case-secs <n>          Per-question timeout (default: 30)");
}

fn load_question_file(path: &Path) -> Result<UsabilityQuestionFile, AnyError> {
    let raw = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read question file {}: {err}",
            path.display()
        )
    })?;
    Ok(serde_json::from_str(&raw)?)
}

async fn prepare_engine(engine: &MemoriEngine, timeout_secs: u64) -> Result<(), AnyError> {
    timeout(Duration::from_secs(timeout_secs), engine.prepare_retrieval_index()).await??;
    Ok(())
}

async fn run_questions(
    engine: &MemoriEngine,
    args: &CliArgs,
    question_file: &UsabilityQuestionFile,
) -> Result<Vec<UsabilityQuestionResult>, AnyError> {
    let mut results = Vec::with_capacity(question_file.questions.len());

    for question in &question_file.questions {
        let scope_paths = normalize_scope_paths(&args.corpus_root, &question.scope_paths);
        let ask_result = timeout(
            Duration::from_secs(args.max_case_secs),
            engine.ask_structured(
                &question.question,
                args.lang.as_deref(),
                if scope_paths.is_empty() {
                    None
                } else {
                    Some(scope_paths.as_slice())
                },
                Some(args.top_k),
            ),
        )
        .await;

        let result = match ask_result {
            Ok(Ok(response)) => evaluate_response(question, response),
            Ok(Err(err)) => UsabilityQuestionResult {
                id: question.id.clone(),
                question: question.question.clone(),
                mode: question.mode,
                status: AskStatus::InsufficientEvidence,
                pass: false,
                failure_class: Some(FailureClass::RetrievalMiss),
                reasons: vec![format!("ask failed: {err}")],
                answer: String::new(),
                citations: Vec::new(),
                citation_valid: false,
                evidence_count: 0,
                notes: question.notes.clone(),
            },
            Err(_) => UsabilityQuestionResult {
                id: question.id.clone(),
                question: question.question.clone(),
                mode: question.mode,
                status: AskStatus::InsufficientEvidence,
                pass: false,
                failure_class: Some(FailureClass::RetrievalMiss),
                reasons: vec![format!("case timed out after {}s", args.max_case_secs)],
                answer: String::new(),
                citations: Vec::new(),
                citation_valid: false,
                evidence_count: 0,
                notes: question.notes.clone(),
            },
        };
        results.push(result);
    }

    Ok(results)
}

fn evaluate_response(
    question: &UsabilityQuestion,
    response: AskResponseStructured,
) -> UsabilityQuestionResult {
    let answer = response.answer.trim().to_string();
    let citation_paths = response
        .citations
        .iter()
        .map(|item| item.relative_path.clone())
        .collect::<Vec<_>>();
    let citation_valid = response
        .citations
        .iter()
        .any(|item| Path::new(&item.file_path).exists());

    let mut reasons = Vec::new();
    let mut failure_class = None;
    let mut pass = true;

    match question.mode {
        QuestionMode::Answer => {
            if response.status != AskStatus::Answered {
                pass = false;
                failure_class = Some(if !response.citations.is_empty() || !response.evidence.is_empty() {
                    FailureClass::GatingFalseRefusal
                } else {
                    FailureClass::RetrievalMiss
                });
                reasons.push(format!("expected answered status, got {:?}", response.status));
            }
            if answer_indicates_insufficient_evidence(&answer) {
                pass = false;
                failure_class.get_or_insert(FailureClass::AnswerSynthesisFail);
                reasons.push("answer still admits insufficient context/evidence".to_string());
            }
            if response.citations.is_empty() {
                pass = false;
                failure_class.get_or_insert(FailureClass::RetrievalMiss);
                reasons.push("no citations returned".to_string());
            }
            if !citation_valid {
                pass = false;
                failure_class.get_or_insert(FailureClass::RetrievalMiss);
                reasons.push("no citation points to an existing local file".to_string());
            }
            if !question.expected_citation_suffixes.is_empty()
                && !response.citations.iter().any(|citation| {
                    let rel = citation.relative_path.replace('\\', "/").to_ascii_lowercase();
                    question.expected_citation_suffixes.iter().any(|suffix| {
                        rel.ends_with(&suffix.replace('\\', "/").to_ascii_lowercase())
                    })
                })
            {
                pass = false;
                failure_class.get_or_insert(FailureClass::RetrievalMiss);
                reasons.push("citations did not hit any expected target file".to_string());
            }
            let answer_lower = answer.to_ascii_lowercase();
            let missing_phrases = question
                .expected_answer_contains
                .iter()
                .filter(|phrase| !answer_lower.contains(&phrase.to_ascii_lowercase()))
                .cloned()
                .collect::<Vec<_>>();
            if !missing_phrases.is_empty() {
                pass = false;
                failure_class.get_or_insert(FailureClass::AnswerSynthesisFail);
                reasons.push(format!(
                    "answer missing expected phrases: {}",
                    missing_phrases.join(", ")
                ));
            }
            let forbidden_hits = question
                .forbidden_answer_contains
                .iter()
                .filter(|phrase| answer_lower.contains(&phrase.to_ascii_lowercase()))
                .cloned()
                .collect::<Vec<_>>();
            if !forbidden_hits.is_empty() {
                pass = false;
                failure_class.get_or_insert(FailureClass::AnswerSynthesisFail);
                reasons.push(format!(
                    "answer contains forbidden phrases: {}",
                    forbidden_hits.join(", ")
                ));
            }
        }
        QuestionMode::Refuse => {
            if response.status == AskStatus::Answered && !answer_indicates_insufficient_evidence(&answer) {
                pass = false;
                failure_class = Some(FailureClass::AnswerSynthesisFail);
                reasons.push("expected refusal/insufficient evidence but got substantive answer".to_string());
            }
        }
    }

    UsabilityQuestionResult {
        id: question.id.clone(),
        question: question.question.clone(),
        mode: question.mode,
        status: response.status,
        pass,
        failure_class,
        reasons,
        answer,
        citations: citation_paths,
        citation_valid,
        evidence_count: response.evidence.len(),
        notes: question.notes.clone(),
    }
}

fn summarize_results(results: &[UsabilityQuestionResult], corpus_document_count: usize) -> UsabilitySummary {
    let question_count = results.len();
    let answer_questions = results
        .iter()
        .filter(|item| item.mode == QuestionMode::Answer)
        .count();
    let refuse_questions = question_count.saturating_sub(answer_questions);
    let usable_answer_count = results
        .iter()
        .filter(|item| item.mode == QuestionMode::Answer && item.pass)
        .count();
    let pass_count = results.iter().filter(|item| item.pass).count();
    let fail_count = question_count.saturating_sub(pass_count);
    let pass_rate = if question_count == 0 {
        0.0
    } else {
        pass_count as f64 / question_count as f64
    };
    let no_false_answered_refusal = results.iter().all(|item| {
        !(item.status == AskStatus::Answered && answer_indicates_insufficient_evidence(&item.answer))
    });

    UsabilitySummary {
        question_count,
        answer_questions,
        refuse_questions,
        usable_answer_count,
        pass_count,
        fail_count,
        pass_rate,
        gate_passed: usable_answer_count >= 10 && no_false_answered_refusal,
        corpus_target_met: corpus_document_count >= 10,
        no_false_answered_refusal,
    }
}

fn render_markdown_report(report: &UsabilityReport) -> String {
    let mut output = String::new();
    output.push_str("# Usability Smoke Report\n\n");
    output.push_str(&format!("- generated_at_utc: `{}`\n", report.generated_at_utc));
    output.push_str(&format!("- corpus_root: `{}`\n", report.corpus_root));
    output.push_str(&format!("- questions_path: `{}`\n", report.questions_path));
    output.push_str(&format!("- db_path: `{}`\n", report.db_path));
    output.push_str(&format!("- indexed_documents: `{}`\n", report.baseline.indexed_document_count));
    output.push_str(&format!("- corpus_document_count: `{}`\n", report.corpus_document_count));
    output.push_str(&format!("- usable_answer_count: `{}/{}`\n", report.summary.usable_answer_count, report.summary.question_count));
    output.push_str(&format!("- gate_passed: `{}`\n", yes_no(report.summary.gate_passed)));
    output.push_str(&format!("- corpus_target_met: `{}`\n\n", yes_no(report.summary.corpus_target_met)));
    output.push_str("| ID | Mode | Status | Pass | Failure | Citations | Reasons |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for result in &report.results {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | {} |\n",
            result.id,
            question_mode_as_str(result.mode),
            ask_status_as_str(result.status),
            yes_no(result.pass),
            result
                .failure_class
                .map(failure_class_as_str)
                .unwrap_or("-"),
            if result.citations.is_empty() {
                "-".to_string()
            } else {
                result.citations.join("<br>")
            },
            if result.reasons.is_empty() {
                "-".to_string()
            } else {
                result.reasons.join("<br>")
            }
        ));
    }
    output
}

fn normalize_scope_paths(corpus_root: &Path, scope_paths: &[String]) -> Vec<PathBuf> {
    scope_paths
        .iter()
        .map(|path| {
            let candidate = PathBuf::from(path);
            if candidate.is_absolute() {
                candidate
            } else {
                corpus_root.join(candidate)
            }
        })
        .collect()
}

fn count_supported_documents(root: &Path) -> Result<usize, AnyError> {
    let mut count = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if is_supported_index_file(&path) {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn is_supported_index_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "txt"))
        .unwrap_or(false)
}

fn question_mode_as_str(mode: QuestionMode) -> &'static str {
    match mode {
        QuestionMode::Answer => "answer",
        QuestionMode::Refuse => "refuse",
    }
}

fn ask_status_as_str(status: AskStatus) -> &'static str {
    match status {
        AskStatus::Answered => "answered",
        AskStatus::InsufficientEvidence => "insufficient_evidence",
        AskStatus::ModelFailedWithEvidence => "model_failed_with_evidence",
    }
}

fn failure_class_as_str(class: FailureClass) -> &'static str {
    match class {
        FailureClass::RetrievalMiss => "retrieval_miss",
        FailureClass::GatingFalseRefusal => "gating_false_refusal",
        FailureClass::AnswerSynthesisFail => "answer_synthesis_fail",
    }
}

fn answer_indicates_insufficient_evidence(answer: &str) -> bool {
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    [
        "当前上下文不足",
        "上下文不足",
        "证据不足",
        "insufficient context",
        "not enough context",
        "insufficient evidence",
        "not enough evidence",
        "lack sufficient context",
        "lack sufficient evidence",
    ]
    .iter()
    .any(|marker| trimmed.contains(marker) || lower.contains(marker))
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}

fn unix_timestamp_string() -> String {
    unix_timestamp_secs().to_string()
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
