use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tokio::runtime::Handle;

const OUTPUT_TAIL_LIMIT: usize = 12_000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RetrievalRegressionReportEntry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) json_path: String,
    pub(crate) md_path: Option<String>,
    pub(crate) report_schema_version: String,
    pub(crate) app_version: String,
    pub(crate) suite_version: String,
    pub(crate) mode: String,
    pub(crate) profile: String,
    pub(crate) generated_at_utc: String,
    pub(crate) service_health: String,
    pub(crate) rerank_health: String,
    pub(crate) case_count: usize,
    pub(crate) last_modified_ms: u128,
    pub(crate) summary: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunRetrievalRegressionPayload {
    pub(crate) mode: String,
    pub(crate) profile: String,
    pub(crate) suite: Option<String>,
    pub(crate) watch_root: Option<String>,
    pub(crate) db_path: Option<String>,
    pub(crate) case_filter: Option<String>,
    pub(crate) max_index_prep_secs: Option<u64>,
    pub(crate) max_case_secs: Option<u64>,
}

#[tauri::command]
pub(crate) async fn list_retrieval_regression_reports()
-> Result<Vec<RetrievalRegressionReportEntry>, String> {
    list_reports()
}

#[tauri::command]
pub(crate) async fn read_retrieval_regression_report(report_path: String) -> Result<Value, String> {
    let repo_root = resolve_repo_root()?;
    let report_root = repo_root.join("target").join("retrieval-regression");
    let requested = PathBuf::from(report_path);
    let canonical_report_root = report_root.canonicalize().map_err(|err| {
        format!(
            "failed to locate report root {}: {err}",
            report_root.display()
        )
    })?;
    let canonical_requested = requested.canonicalize().map_err(|err| {
        format!(
            "failed to locate report file {}: {err}",
            requested.display()
        )
    })?;

    if !canonical_requested.starts_with(&canonical_report_root) {
        return Err("report path must stay under target/retrieval-regression".to_string());
    }

    let raw = fs::read_to_string(&canonical_requested).map_err(|err| {
        format!(
            "failed to read report {}: {err}",
            canonical_requested.display()
        )
    })?;
    serde_json::from_str(&raw).map_err(|err| {
        format!(
            "failed to parse report json {}: {err}",
            canonical_requested.display()
        )
    })
}

#[tauri::command]
pub(crate) async fn get_retrieval_regression_progress(
    _run_id: Option<String>,
) -> Result<Option<Value>, String> {
    let repo_root = resolve_repo_root()?;
    let progress_path = regression_progress_path(&repo_root);
    let Ok(raw) = fs::read_to_string(progress_path) else {
        return Ok(None);
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub(crate) async fn run_retrieval_regression(
    payload: RunRetrievalRegressionPayload,
    state: State<'_, DesktopState>,
) -> Result<RetrievalRegressionRunState, String> {
    validate_mode(&payload.mode)?;
    validate_profile(&payload.profile)?;

    let active_run = {
        let runs = state.regression_runs.lock().await;
        runs.values().find(|run| run.status == "running").cloned()
    };
    if let Some(run) = active_run {
        return Err(format!(
            "a regression run is already active: {} / {} / {}",
            run.id, run.mode, run.profile
        ));
    }

    let repo_root = resolve_repo_root()?;
    let script_path = repo_root.join("scripts").join("test-retrieval.ps1");
    if !script_path.exists() {
        return Err(format!(
            "missing regression script: {}",
            script_path.display()
        ));
    }

    let _ = fs::remove_file(regression_progress_path(&repo_root));

    let run_id = format!("retrieval-{}", now_ms());
    let started = RetrievalRegressionRunState {
        id: run_id.clone(),
        status: "running".to_string(),
        mode: payload.mode.clone(),
        profile: payload.profile.clone(),
        case_filter: payload
            .case_filter
            .clone()
            .filter(|value| !value.trim().is_empty()),
        started_at_ms: now_ms(),
        finished_at_ms: None,
        exit_code: None,
        report_path: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        error: None,
    };

    {
        let mut runs = state.regression_runs.lock().await;
        runs.insert(run_id.clone(), started.clone());
    }

    let runs = Arc::clone(&state.regression_runs);
    let runtime = Handle::current();
    tauri::async_runtime::spawn(async move {
        let runs_for_process = Arc::clone(&runs);
        let process_run_id = run_id.clone();
        let finished = tokio::task::spawn_blocking(move || {
            run_regression_process(
                repo_root,
                script_path,
                payload,
                runs_for_process,
                process_run_id,
                runtime,
            )
        })
        .await
        .unwrap_or_else(|err| Err(format!("regression task join failed: {err}")));

        let mut runs = runs.lock().await;
        if let Some(run) = runs.get_mut(&run_id) {
            run.finished_at_ms = Some(now_ms());
            match finished {
                Ok(process_result) => {
                    run.status = if process_result.exit_code == Some(0) {
                        "succeeded".to_string()
                    } else {
                        "failed".to_string()
                    };
                    run.exit_code = process_result.exit_code;
                    run.report_path = process_result.report_path;
                    run.stdout_tail = process_result.stdout_tail;
                    run.stderr_tail = process_result.stderr_tail;
                    if run.status == "failed" {
                        run.error =
                            Some("regression process exited with a non-zero code".to_string());
                    }
                }
                Err(err) => {
                    run.status = "failed".to_string();
                    run.error = Some(err);
                }
            }
        }
    });

    Ok(started)
}

#[tauri::command]
pub(crate) async fn get_retrieval_regression_run(
    run_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Option<RetrievalRegressionRunState>, String> {
    let runs = state.regression_runs.lock().await;
    if let Some(run_id) = run_id {
        return Ok(runs.get(&run_id).cloned());
    }
    Ok(runs.values().max_by_key(|run| run.started_at_ms).cloned())
}

struct ProcessResult {
    exit_code: Option<i32>,
    report_path: Option<String>,
    stdout_tail: String,
    stderr_tail: String,
}

fn run_regression_process(
    repo_root: PathBuf,
    script_path: PathBuf,
    payload: RunRetrievalRegressionPayload,
    runs: Arc<tokio::sync::Mutex<std::collections::HashMap<String, RetrievalRegressionRunState>>>,
    run_id: String,
    runtime: Handle,
) -> Result<ProcessResult, String> {
    let mode = payload.mode.clone();
    let profile = payload.profile.clone();
    let mut command = Command::new("powershell.exe");
    command
        .current_dir(&repo_root)
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(script_path)
        .arg("-Mode")
        .arg(&mode)
        .arg("-Profile")
        .arg(&profile)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(value) = clean_optional(payload.suite) {
        command.arg("-Suite").arg(value);
    }
    if let Some(value) = clean_optional(payload.watch_root) {
        command.arg("-WatchRoot").arg(value);
    }
    if let Some(value) = clean_optional(payload.db_path) {
        command.arg("-DbPath").arg(value);
    }
    if let Some(value) = clean_optional(payload.case_filter) {
        command.arg("-Case").arg(value);
    }
    if let Some(value) = payload.max_index_prep_secs {
        command.arg("-MaxIndexPrepSecs").arg(value.to_string());
    }
    if let Some(value) = payload.max_case_secs {
        command.arg("-MaxCaseSecs").arg(value.to_string());
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to start regression powershell script: {err}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("regression process did not expose stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or("regression process did not expose stderr".to_string())?;

    let (tx, rx) = mpsc::channel::<(bool, String)>();
    let stdout_reader = spawn_output_reader(stdout, true, tx.clone());
    let stderr_reader = spawn_output_reader(stderr, false, tx);
    let mut stdout_tail = String::new();
    let mut stderr_tail = String::new();

    loop {
        while let Ok((is_stdout, chunk)) = rx.try_recv() {
            if is_stdout {
                stdout_tail.push_str(&chunk);
                stdout_tail = tail_text(&stdout_tail);
            } else {
                stderr_tail.push_str(&chunk);
                stderr_tail = tail_text(&stderr_tail);
            }
            update_run_output(&runtime, &runs, &run_id, &stdout_tail, &stderr_tail);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                while let Ok((is_stdout, chunk)) = rx.try_recv() {
                    if is_stdout {
                        stdout_tail.push_str(&chunk);
                        stdout_tail = tail_text(&stdout_tail);
                    } else {
                        stderr_tail.push_str(&chunk);
                        stderr_tail = tail_text(&stderr_tail);
                    }
                }
                update_run_output(&runtime, &runs, &run_id, &stdout_tail, &stderr_tail);

                let report_path = latest_report_path(&repo_root, &mode, &profile);
                return Ok(ProcessResult {
                    exit_code: status.code(),
                    report_path,
                    stdout_tail,
                    stderr_tail,
                });
            }
            Ok(None) => thread::sleep(std::time::Duration::from_millis(150)),
            Err(err) => {
                return Err(format!(
                    "failed while waiting for regression process: {err}"
                ));
            }
        }
    }
}

fn list_reports() -> Result<Vec<RetrievalRegressionReportEntry>, String> {
    let repo_root = resolve_repo_root()?;
    let report_root = repo_root.join("target").join("retrieval-regression");
    if !report_root.exists() {
        return Ok(Vec::new());
    }

    let mut reports = Vec::new();
    for entry in fs::read_dir(&report_root).map_err(|err| {
        format!(
            "failed to read report directory {}: {err}",
            report_root.display()
        )
    })? {
        let entry = entry.map_err(|err| format!("failed to read report entry: {err}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let json_path = path.join("report.json");
        if !json_path.exists() {
            continue;
        }
        let raw = match fs::read_to_string(&json_path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let Ok(report) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let metadata = entry.metadata().ok();
        let last_modified_ms = metadata
            .and_then(|m| m.modified().ok())
            .and_then(system_time_ms)
            .unwrap_or(0);
        let name = entry.file_name().to_string_lossy().to_string();
        let summary = report.get("summary").cloned().unwrap_or(Value::Null);
        let case_count = summary
            .get("case_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;

        reports.push(RetrievalRegressionReportEntry {
            id: name.clone(),
            name,
            path: path.to_string_lossy().to_string(),
            json_path: json_path.to_string_lossy().to_string(),
            md_path: path
                .join("report.md")
                .exists()
                .then(|| path.join("report.md").to_string_lossy().to_string()),
            report_schema_version: json_string(&report, "report_schema_version"),
            app_version: json_string(&report, "app_version"),
            suite_version: json_scalar_string(&report, "suite_version"),
            mode: json_string(&report, "evaluation_mode"),
            profile: json_string(&report, "profile"),
            generated_at_utc: json_string(&report, "generated_at_utc"),
            service_health: json_string(&report, "service_health"),
            rerank_health: json_string(&report, "rerank_health"),
            case_count,
            last_modified_ms,
            summary,
        });
    }

    reports.sort_by_key(|item| Reverse(item.last_modified_ms));
    Ok(reports)
}

fn latest_report_path(repo_root: &Path, mode: &str, profile: &str) -> Option<String> {
    let report_root = repo_root.join("target").join("retrieval-regression");
    let prefix = format!("{mode}-{profile}-");
    let mut entries = fs::read_dir(report_root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().is_dir() && entry.file_name().to_string_lossy().starts_with(&prefix)
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path().join("report.json")))
        })
        .filter(|(_, path)| path.exists())
        .collect::<Vec<_>>();
    entries.sort_by_key(|(modified, _)| *modified);
    entries
        .pop()
        .map(|(_, path)| path.to_string_lossy().to_string())
}

fn regression_progress_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join("target")
        .join("retrieval-regression")
        .join(".active-progress.json")
}

fn resolve_repo_root() -> Result<PathBuf, String> {
    let mut current =
        std::env::current_dir().map_err(|err| format!("failed to read current dir: {err}"))?;
    loop {
        if current.join("Cargo.toml").exists()
            && current.join("scripts").join("test-retrieval.ps1").exists()
        {
            return Ok(current);
        }
        if !current.pop() {
            return Err("failed to locate repository root from current directory".to_string());
        }
    }
}

fn validate_mode(value: &str) -> Result<(), String> {
    match value {
        "offline_deterministic" | "live_embedding" => Ok(()),
        _ => Err(format!("unsupported regression mode: {value}")),
    }
}

fn validate_profile(value: &str) -> Result<(), String> {
    match value {
        "core_docs" | "repo_mixed" | "full_live" => Ok(()),
        _ => Err(format!("unsupported regression profile: {value}")),
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn json_string(report: &Value, key: &str) -> String {
    report
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string()
}

fn json_scalar_string(report: &Value, key: &str) -> String {
    report
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .unwrap_or_else(|| value.to_string())
        })
        .unwrap_or_default()
}

fn system_time_ms(time: SystemTime) -> Option<u128> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis())
}

fn now_ms() -> u128 {
    system_time_ms(SystemTime::now()).unwrap_or(0)
}

fn tail_text(text: &str) -> String {
    if text.len() <= OUTPUT_TAIL_LIMIT {
        return text.to_string();
    }
    let mut start = text.len() - OUTPUT_TAIL_LIMIT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("...{}", &text[start..])
}

fn spawn_output_reader(
    stream: impl std::io::Read + Send + 'static,
    is_stdout: bool,
    tx: mpsc::Sender<(bool, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if tx.send((is_stdout, format!("{line}\n"))).is_err() {
                break;
            }
        }
    })
}

fn update_run_output(
    runtime: &Handle,
    runs: &Arc<tokio::sync::Mutex<std::collections::HashMap<String, RetrievalRegressionRunState>>>,
    run_id: &str,
    stdout_tail: &str,
    stderr_tail: &str,
) {
    let run_id = run_id.to_string();
    let stdout_tail = stdout_tail.to_string();
    let stderr_tail = stderr_tail.to_string();
    runtime.block_on(async {
        let mut guard = runs.lock().await;
        if let Some(run) = guard.get_mut(&run_id) {
            run.stdout_tail = stdout_tail;
            run.stderr_tail = stderr_tail;
        }
    });
}
