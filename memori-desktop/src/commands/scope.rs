use super::*;

#[tauri::command]
pub(crate) async fn set_watch_root(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<AppSettingsDto, String> {
    info!(path = %path, "[用户操作] 设置监听目录");
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("目录路径为空，无法保存。".to_string());
    }

    let watch_root = PathBuf::from(trimmed);
    if !watch_root.exists() {
        return Err(format!("目录不存在: {}", watch_root.display()));
    }
    if !watch_root.is_dir() {
        return Err(format!("路径不是目录: {}", watch_root.display()));
    }

    let canonical = watch_root
        .canonicalize()
        .map_err(|err| format!("规范化目录失败: {err}"))?;

    let mut settings = load_app_settings()?;
    settings.watch_root = Some(canonical.to_string_lossy().to_string());
    save_app_settings(&settings)?;

    replace_engine(
        &state.engine,
        &state.init_error,
        canonical.clone(),
        "settings_watch_root_update",
    )
    .await?;

    let indexing = resolve_indexing_config(&settings);
    Ok(AppSettingsDto::from_settings(
        settings,
        canonical.to_string_lossy().to_string(),
        indexing,
    ))
}

#[tauri::command]
pub(crate) async fn list_search_scopes() -> Result<Vec<SearchScopeItem>, String> {
    info!("[用户操作] 获取搜索范围列表");
    let settings = load_app_settings()?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    collect_search_scopes(&watch_root)
}

#[tauri::command]
pub(crate) async fn open_source_location(path: String) -> Result<(), String> {
    info!(path = %path, "[用户操作] 打开文件位置");
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("文件路径为空，无法打开。".to_string());
    }

    let target = PathBuf::from(trimmed);
    if !target.exists() {
        return Err(format!("文件不存在: {}", target.display()));
    }

    #[cfg(target_os = "windows")]
    {
        let canonical = target.canonicalize().unwrap_or_else(|_| target.clone());
        let normalized = canonical.to_string_lossy().replace('/', "\\");
        if canonical.is_file() {
            if let Err(first_err) = Command::new("explorer.exe")
                .arg("/select,")
                .arg(&normalized)
                .spawn()
            {
                Command::new("explorer.exe")
                    .arg(format!("/select,\"{normalized}\""))
                    .spawn()
                    .map_err(|fallback_err| {
                        format!("打开文件位置失败: {first_err}; fallback: {fallback_err}")
                    })?;
            }
        } else {
            Command::new("explorer.exe")
                .arg(&normalized)
                .spawn()
                .map_err(|err| format!("打开文件位置失败: {err}"))?;
        }
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .arg("-R")
            .arg(&target)
            .status()
            .map_err(|err| format!("打开文件位置失败: {err}"))?;
        if !status.success() {
            return Err("打开文件位置失败: open 返回非零状态".to_string());
        }
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let open_path = if target.is_file() {
            target
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        } else {
            target
        };
        let status = Command::new("xdg-open")
            .arg(open_path)
            .status()
            .map_err(|err| format!("打开文件位置失败: {err}"))?;
        if !status.success() {
            return Err("打开文件位置失败: xdg-open 返回非零状态".to_string());
        }
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("当前系统暂不支持打开文件位置".to_string())
}

#[tauri::command]
pub(crate) async fn read_file_content(path: String) -> Result<String, String> {
    read_file_preview(path).await.map(|preview| preview.content)
}

#[tauri::command]
pub(crate) async fn read_file_preview(path: String) -> Result<FilePreviewDto, String> {
    info!(path = %path, "[用户操作] 预览文件");
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("文件路径为空".to_string());
    }

    let target = PathBuf::from(trimmed);
    if !target.exists() {
        return Err(format!("文件不存在: {}", target.display()));
    }
    if !target.is_file() {
        return Err(format!("路径不是文件: {}", target.display()));
    }

    // Only allow previewing supported document/text files for safety.
    let ext = target
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let plain_text_exts = [
        "md", "txt", "rs", "py", "js", "ts", "jsx", "tsx", "json", "yaml", "yml", "toml", "html",
        "css", "c", "cpp", "h", "hpp", "go", "java", "kt", "swift", "rb", "php", "sh", "bat",
        "ps1", "log",
    ];
    let extracted_text_exts = ["docx", "pdf"];
    if !plain_text_exts.contains(&ext.as_str()) && !extracted_text_exts.contains(&ext.as_str()) {
        return Err(format!("不支持预览的文件类型: .{ext}"));
    }

    // Read with size limit (5MB)
    let metadata = std::fs::metadata(&target).map_err(|e| format!("读取文件元数据失败: {e}"))?;
    if metadata.len() > 5 * 1024 * 1024 {
        return Err("文件过大（超过 5MB）".to_string());
    }

    let content = if extracted_text_exts.contains(&ext.as_str()) {
        memori_parser::extract_document_text(&target)
            .ok_or_else(|| format!("无法从 .{ext} 文件提取可预览文本"))?
    } else {
        std::fs::read_to_string(&target).map_err(|e| format!("读取文件失败: {e}"))?
    };
    let format = match ext.as_str() {
        "md" => "markdown",
        "docx" | "pdf" => "document",
        _ => "text",
    };
    Ok(FilePreviewDto {
        content,
        format: format.to_string(),
        extension: ext,
    })
}
