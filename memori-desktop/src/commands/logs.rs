use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub target: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub thread_id: Option<String>,
}

fn log_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("Memori-Vault").join("logs"))
        .ok_or_else(|| "无法获取日志目录".to_string())
}

#[tauri::command]
pub(crate) async fn get_logs(
    limit: Option<usize>,
    level_filter: Option<String>,
) -> Result<Vec<LogEntry>, String> {
    let log_dir = log_dir()?;
    if !log_dir.exists() {
        return Ok(Vec::new());
    }

    let max = limit.unwrap_or(500);
    let filter = level_filter.map(|s| s.to_ascii_uppercase());
    let mut files: Vec<_> = std::fs::read_dir(&log_dir)
        .map_err(|e| format!("读取日志目录失败: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("memori.log"))
        .collect();

    files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    files.reverse();

    let mut entries = Vec::new();
    for entry in files {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("读取日志文件失败({}): {e}", path.display()))?;

        for line in content.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(log_entry) = parse_log_line(line) else {
                continue;
            };
            if let Some(ref f) = filter
                && log_entry.level.to_ascii_uppercase() != *f
            {
                continue;
            }
            entries.push(log_entry);
            if entries.len() >= max {
                break;
            }
        }
        if entries.len() >= max {
            break;
        }
    }

    entries.reverse();
    Ok(entries)
}

#[tauri::command]
pub(crate) async fn get_log_dir() -> Result<String, String> {
    let dir = log_dir()?;
    Ok(dir.to_string_lossy().to_string())
}

fn parse_log_line(line: &str) -> Result<LogEntry, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(line)?;
    let fields = value.get("fields").and_then(|v| v.as_object());
    let message = fields.map(format_log_message).unwrap_or_else(|| {
        value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    });
    let target = value
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let file = value
        .get("file")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let category = classify_log_category(&target, &message, file.as_deref(), fields);

    Ok(LogEntry {
        timestamp: value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        level: value
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("INFO")
            .to_string(),
        category,
        target,
        message,
        file,
        line: value.get("line").and_then(|v| v.as_u64()).map(|n| n as u32),
        thread_id: value
            .get("threadId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn classify_log_category(
    target: &str,
    message: &str,
    file: Option<&str>,
    fields: Option<&serde_json::Map<String, serde_json::Value>>,
) -> String {
    if is_mcp_log(target, message, file, fields) {
        "MCP".to_string()
    } else {
        "SYSTEM".to_string()
    }
}

fn is_mcp_log(
    target: &str,
    message: &str,
    file: Option<&str>,
    fields: Option<&serde_json::Map<String, serde_json::Value>>,
) -> bool {
    if contains_mcp_marker(target) || contains_mcp_marker(message) {
        return true;
    }

    if let Some(file) = file
        && contains_mcp_marker(&file.replace('\\', "/"))
    {
        return true;
    }

    fields
        .map(|fields| {
            fields
                .iter()
                .any(|(key, value)| contains_mcp_marker(key) || json_value_contains_mcp(value))
        })
        .unwrap_or(false)
}

fn json_value_contains_mcp(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => contains_mcp_marker(text),
        serde_json::Value::Array(items) => items.iter().any(json_value_contains_mcp),
        serde_json::Value::Object(map) => map
            .iter()
            .any(|(key, value)| contains_mcp_marker(key) || json_value_contains_mcp(value)),
        _ => false,
    }
}

fn contains_mcp_marker(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("mcp")
        || normalized.contains("model context protocol")
        || normalized.contains("jsonrpc")
        || normalized.contains("json-rpc")
}

fn format_log_message(fields: &serde_json::Map<String, serde_json::Value>) -> String {
    let base = fields
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("日志事件");
    let mut parts = vec![humanize_message(base).to_string()];

    for key in [
        "role",
        "status",
        "reason",
        "error",
        "path",
        "model",
        "port",
        "endpoint",
        "pid",
        "doc_count",
        "chunk_count",
        "evidence_count",
        "ms",
    ] {
        if let Some(value) = fields.get(key).and_then(simple_json_value) {
            parts.push(format!("{}={}", humanize_key(key), value));
        }
    }

    parts.join(" | ")
}

fn simple_json_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn humanize_key(key: &str) -> String {
    match key {
        "role" => "角色".to_string(),
        "status" => "状态".to_string(),
        "reason" => "原因".to_string(),
        "error" => "错误".to_string(),
        "path" => "路径".to_string(),
        "model" => "模型".to_string(),
        "port" => "端口".to_string(),
        "endpoint" => "地址".to_string(),
        "pid" => "进程".to_string(),
        "doc_count" => "文档数".to_string(),
        "chunk_count" => "分块数".to_string(),
        "evidence_count" => "证据数".to_string(),
        "ms" => "耗时ms".to_string(),
        _ => key.to_string(),
    }
}

fn humanize_message(message: &str) -> &str {
    match message {
        "started local llama.cpp model" => "本地模型已启动",
        "local model start failed" => "本地模型启动失败",
        "stopped local llama.cpp model" => "本地模型已停止",
        "failed to stop local llama.cpp model" => "停止本地模型失败",
        "local llama.cpp port conflict" => "端口被占用，模型启动失败",
        "search started" => "开始搜索",
        "search completed" => "搜索完成",
        "search failed" => "搜索失败",
        "indexing embedding started" => "开始构建索引",
        "indexing embedding finished" => "索引向量构建完成",
        "embedding batch failed" => "索引向量生成失败",
        "embedding batch timed out" => "索引向量生成超时",
        "document index write failed" => "索引写入数据库失败",
        "chunk searches completed" => "检索完成",
        "gating blocked answer as insufficient evidence" => "证据不足，已拒答",
        _ => message,
    }
}
