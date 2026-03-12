use crate::*;

pub(crate) async fn append_audit_event(state: &ServerState, event: AuditEventDto) {
    let _guard = state.audit_file_lock.lock().await;
    let path = match audit_log_file_path() {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "解析审计日志路径失败");
            return;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        warn!(error = %err, path = %parent.display(), "创建审计日志目录失败");
        return;
    }
    let line = match serde_json::to_string(&event) {
        Ok(line) => line,
        Err(err) => {
            warn!(error = %err, "序列化审计事件失败");
            return;
        }
    };
    let mut file = match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(err) => {
            warn!(error = %err, path = %path.display(), "打开审计日志文件失败");
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        warn!(error = %err, path = %path.display(), "写入审计日志失败");
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

pub(crate) fn read_audit_events() -> Result<Vec<AuditEventDto>, String> {
    let path = audit_log_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path)
        .map_err(|err| format!("打开审计日志失败({}): {err}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|err| format!("读取审计日志失败({}): {err}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<AuditEventDto>(trimmed) {
            events.push(event);
        }
    }
    events.sort_by_key(|event| std::cmp::Reverse(event.timestamp));
    Ok(events)
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
    let config_root = dirs::config_dir().ok_or_else(|| "无法获取用户配置目录".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(AUDIT_LOG_FILE_NAME))
}
