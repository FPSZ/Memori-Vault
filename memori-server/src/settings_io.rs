use crate::*;

pub(crate) fn app_settings_file_path() -> Result<PathBuf, String> {
    let config_root = dirs::config_dir().ok_or_else(|| "无法获取用户配置目录".to_string())?;
    Ok(config_root
        .join(SETTINGS_APP_DIR_NAME)
        .join(SETTINGS_FILE_NAME))
}

pub(crate) fn load_app_settings() -> Result<AppSettings, String> {
    let settings_file = app_settings_file_path()?;
    if !settings_file.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&settings_file)
        .map_err(|err| format!("读取配置失败({}): {err}", settings_file.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("解析配置失败({}): {err}", settings_file.display()))
}

pub(crate) fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
    let settings_file = app_settings_file_path()?;
    if let Some(parent) = settings_file.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("创建配置目录失败({}): {err}", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(settings).map_err(|err| format!("序列化配置失败: {err}"))?;
    fs::write(&settings_file, content)
        .map_err(|err| format!("写入配置失败({}): {err}", settings_file.display()))
}

pub(crate) fn resolve_watch_root_from_settings(settings: &AppSettings) -> Result<PathBuf, String> {
    if let Some(path) = settings.watch_root.as_deref() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(path) = std::env::var("MEMORI_WATCH_ROOT") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    std::env::current_dir().map_err(|err| format!("获取当前工作目录失败: {err}"))
}
