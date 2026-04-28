use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub thread_id: Option<String>,
}

fn log_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("Memori-Vault").join("logs"))
        .ok_or_else(|| "无法获取配置目录".to_string())
}

/// 读取最近的日志条目。
/// `limit`: 最多返回条数（默认 500）
/// `level_filter`: 可选级别过滤（"TRACE"/"DEBUG"/"INFO"/"WARN"/"ERROR"）
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

    // 收集并按修改时间倒序
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
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("读取日志文件失败: {e}"))?;

        for line in content.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_log_line(line) {
                Ok(log_entry) => {
                    if let Some(ref f) = filter {
                        if log_entry.level.to_ascii_uppercase() != *f {
                            continue;
                        }
                    }
                    entries.push(log_entry);
                    if entries.len() >= max {
                        break;
                    }
                }
                Err(_) => {
                    // 解析失败的行（可能是非 JSON 的尾部）跳过
                    continue;
                }
            }
        }
        if entries.len() >= max {
            break;
        }
    }

    // 按时间正序返回（旧的在前面）
    entries.reverse();
    Ok(entries)
}

/// 获取日志目录路径（供前端显示）
#[tauri::command]
pub(crate) async fn get_log_dir() -> Result<String, String> {
    let dir = log_dir()?;
    Ok(dir.to_string_lossy().to_string())
}

fn parse_log_line(line: &str) -> Result<LogEntry, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(line)?;

    let fields = value.get("fields").and_then(|v| v.as_object());

    // 尝试从 fields 拼接 message：优先用 fields.message，否则把所有字段拼起来
    let message = fields
        .and_then(|f| f.get("message"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            fields
                .map(|f| {
                    f.iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default()
        });

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
        target: value
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        message,
        file: value
            .get("file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        line: value.get("line").and_then(|v| v.as_u64()).map(|n| n as u32),
        thread_id: value
            .get("threadId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}


