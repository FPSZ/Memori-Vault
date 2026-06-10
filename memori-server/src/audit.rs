use std::cmp::Reverse;
use std::collections::VecDeque;

use crate::*;

const MAX_AUDIT_PAGE_SCAN_EVENTS: usize = 10_000;

pub(crate) async fn append_audit_event(state: &ServerState, event: AuditEventDto) {
    let _guard = state.audit_file_lock.lock().await;
    let path = match audit_log_file_path() {
        Ok(path) => path,
        Err(err) => {
            error!(error = %err, "audit event dropped: failed to resolve audit log path");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        error!(error = %err, path = %parent.display(), "audit event dropped: failed to create audit log directory");
        return;
    }
    let line = match serde_json::to_string(&event) {
        Ok(line) => line,
        Err(err) => {
            error!(error = %err, "audit event dropped: failed to serialize audit event");
            return;
        }
    };
    let mut file = match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(err) => {
            error!(error = %err, path = %path.display(), "audit event dropped: failed to open audit log file");
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        error!(error = %err, path = %path.display(), "audit event dropped: failed to append to audit log");
    }
}

pub(crate) async fn append_policy_violation_audit(
    state: &ServerState,
    actor: String,
    action: &str,
    provider: Option<ModelProvider>,
    endpoint: Option<&str>,
    models: &[String],
    message: &str,
) {
    append_audit_event(
        state,
        AuditEventDto {
            actor,
            action: "policy_violation".to_string(),
            resource: "model_runtime".to_string(),
            timestamp: unix_now_secs(),
            result: "blocked".to_string(),
            metadata: serde_json::json!({
                "source_action": action,
                "provider": provider.map(provider_to_string),
                "endpoint": endpoint.map(|value| value.trim()),
                "models": models,
                "message": message,
            }),
        },
    )
    .await;
}

pub(crate) fn read_audit_events_page(
    page: usize,
    page_size: usize,
) -> Result<(usize, Vec<AuditEventDto>), String> {
    let path = audit_log_file_path()?;
    if !path.exists() {
        return Ok((0, Vec::new()));
    }

    let keep = page
        .saturating_mul(page_size.max(1))
        .clamp(page_size.max(1), MAX_AUDIT_PAGE_SCAN_EVENTS);
    let file = fs::File::open(&path)
        .map_err(|err| format!("failed to open audit log ({}): {err}", path.display()))?;
    let reader = BufReader::new(file);
    read_audit_events_page_from_reader(reader, page, page_size, keep, &path)
}

fn read_audit_events_page_from_reader<R: BufRead>(
    reader: R,
    page: usize,
    page_size: usize,
    keep: usize,
    path: &Path,
) -> Result<(usize, Vec<AuditEventDto>), String> {
    let mut total = 0usize;
    let mut recent = VecDeque::with_capacity(keep);

    for line in reader.lines() {
        let line =
            line.map_err(|err| format!("failed to read audit log ({}): {err}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<AuditEventDto>(trimmed) {
            total += 1;
            if recent.len() == keep {
                recent.pop_front();
            }
            recent.push_back(event);
        }
    }

    let mut recent = recent.into_iter().collect::<Vec<_>>();
    recent.sort_by_key(|event| Reverse(event.timestamp));
    let start = (page.saturating_sub(1)).saturating_mul(page_size);
    let items = if start >= recent.len() {
        Vec::new()
    } else {
        recent.into_iter().skip(start).take(page_size).collect()
    };

    Ok((total, items))
}

pub(crate) fn snapshot_metrics(metrics: &ServerMetrics) -> ServerMetricsDto {
    let ask_requests = metrics.ask_requests.load(Ordering::Relaxed);
    let ask_latency_total_ms = metrics.ask_latency_total_ms.load(Ordering::Relaxed);
    let ask_latency_avg_ms = if ask_requests == 0 {
        0.0
    } else {
        ask_latency_total_ms as f64 / ask_requests as f64
    };
    ServerMetricsDto {
        total_requests: metrics.total_requests.load(Ordering::Relaxed),
        failed_requests: metrics.failed_requests.load(Ordering::Relaxed),
        ask_requests,
        ask_failed: metrics.ask_failed.load(Ordering::Relaxed),
        ask_latency_avg_ms,
    }
}

pub(crate) fn audit_log_file_path() -> Result<PathBuf, String> {
    let config_root =
        dirs::config_dir().ok_or_else(|| "failed to resolve user config directory".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(AUDIT_LOG_FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_audit_events_page_only_keeps_requested_window() {
        let path = Path::new("audit.log.jsonl");
        let lines = (0..6)
            .map(|index| {
                serde_json::to_string(&AuditEventDto {
                    actor: format!("user-{index}"),
                    action: "ask".to_string(),
                    resource: "vault".to_string(),
                    timestamp: index,
                    result: "ok".to_string(),
                    metadata: serde_json::json!({}),
                })
                .unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let reader = std::io::Cursor::new(lines);

        let (total, items) = read_audit_events_page_from_reader(reader, 1, 2, 2, path).unwrap();
        assert_eq!(total, 6);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].timestamp, 5);
        assert_eq!(items[1].timestamp, 4);
    }
}
