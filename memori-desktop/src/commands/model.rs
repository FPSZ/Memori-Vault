use super::*;

#[tauri::command]
pub(crate) async fn get_model_settings() -> Result<ModelSettingsDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
pub(crate) async fn get_enterprise_policy() -> Result<EnterprisePolicyDto, String> {
    let settings = load_app_settings()?;
    Ok(resolve_enterprise_policy(&settings))
}

#[tauri::command]
pub(crate) async fn set_enterprise_policy(
    payload: EnterprisePolicyDto,
    state: State<'_, DesktopState>,
) -> Result<EnterprisePolicyDto, String> {
    info!(egress_mode = ?payload.egress_mode, endpoints = ?payload.allowed_model_endpoints.len(), models = ?payload.allowed_models.len(), "set enterprise policy");
    let mut settings = load_app_settings()?;
    settings.enterprise_egress_mode = Some(
        match payload.egress_mode {
            EgressMode::LocalOnly => "local_only",
            EgressMode::Allowlist => "allowlist",
        }
        .to_string(),
    );
    settings.enterprise_allowed_model_endpoints = Some(
        payload
            .allowed_model_endpoints
            .iter()
            .map(|item| normalize_policy_endpoint(item))
            .filter(|item| !item.is_empty())
            .collect(),
    );
    settings.enterprise_allowed_models = Some(
        payload
            .allowed_models
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
    );
    save_app_settings(&settings)?;
    let watch_root = resolve_watch_root_from_settings(&settings)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_policy_update",
    )
    .await?;
    Ok(resolve_enterprise_policy(&settings))
}

#[tauri::command]
pub(crate) async fn set_model_settings(
    payload: ModelSettingsDto,
    state: State<'_, DesktopState>,
) -> Result<ModelSettingsDto, String> {
    info!(provider = %payload.active_provider, "set model settings");
    let mut settings = load_app_settings()?;
    let normalized = normalize_model_settings_payload(payload)?;
    let policy = resolve_enterprise_policy(&settings);
    validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&resolve_active_runtime_settings(&normalized)),
    )
    .map_err(|violation| violation.message)?;
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_chat_endpoint = Some(normalized.local_profile.chat_endpoint.clone());
    settings.local_graph_endpoint = Some(normalized.local_profile.graph_endpoint.clone());
    settings.local_embed_endpoint = Some(normalized.local_profile.embed_endpoint.clone());
    settings.local_endpoint = Some(normalized.local_profile.chat_endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_llama_server_path = normalized.local_profile.llama_server_path.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
    settings.local_chat_model_path = normalized.local_profile.chat_model_path.clone();
    settings.local_graph_model_path = normalized.local_profile.graph_model_path.clone();
    settings.local_embed_model_path = normalized.local_profile.embed_model_path.clone();
    settings.remote_chat_endpoint = Some(normalized.remote_profile.chat_endpoint.clone());
    settings.remote_graph_endpoint = Some(normalized.remote_profile.graph_endpoint.clone());
    settings.remote_embed_endpoint = Some(normalized.remote_profile.embed_endpoint.clone());
    settings.remote_endpoint = Some(normalized.remote_profile.chat_endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
    settings.local_chat_context_length = normalized.local_profile.chat_context_length;
    settings.local_graph_context_length = normalized.local_profile.graph_context_length;
    settings.local_embed_context_length = normalized.local_profile.embed_context_length;
    settings.local_chat_concurrency = normalized.local_profile.chat_concurrency;
    settings.local_graph_concurrency = normalized.local_profile.graph_concurrency;
    settings.local_embed_concurrency = normalized.local_profile.embed_concurrency;
    settings.local_performance_preset = normalized.local_profile.performance_preset.clone();
    settings.local_n_gpu_layers = normalized.local_profile.n_gpu_layers;
    settings.local_batch_size = normalized.local_profile.batch_size;
    settings.local_ubatch_size = normalized.local_profile.ubatch_size;
    settings.local_threads = normalized.local_profile.threads;
    settings.local_threads_batch = normalized.local_profile.threads_batch;
    settings.local_flash_attn = normalized.local_profile.flash_attn;
    settings.local_cache_type_k = normalized.local_profile.cache_type_k.clone();
    settings.local_cache_type_v = normalized.local_profile.cache_type_v.clone();
    settings.remote_chat_context_length = normalized.remote_profile.chat_context_length;
    settings.remote_graph_context_length = normalized.remote_profile.graph_context_length;
    settings.remote_embed_context_length = normalized.remote_profile.embed_context_length;
    settings.remote_chat_concurrency = normalized.remote_profile.chat_concurrency;
    settings.remote_graph_concurrency = normalized.remote_profile.graph_concurrency;
    settings.remote_embed_concurrency = normalized.remote_profile.embed_concurrency;
    save_app_settings(&settings)?;

    let watch_root = resolve_watch_root_from_settings(&settings)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "settings_model_update",
    )
    .await?;

    Ok(normalized)
}

#[tauri::command]
#[allow(non_snake_case)]
pub(crate) async fn list_provider_models(
    provider: String,
    chatEndpoint: String,
    graphEndpoint: String,
    embedEndpoint: String,
    apiKey: Option<String>,
    modelsRoot: Option<String>,
) -> Result<ProviderModelsDto, String> {
    info!(provider = %provider, "list provider models");
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let chat_endpoint = normalize_endpoint(provider, &chatEndpoint);
    let graph_endpoint = normalize_endpoint(provider, &graphEndpoint);
    let embed_endpoint = normalize_endpoint(provider, &embedEndpoint);
    let api_key = normalize_optional_text(apiKey);
    let models_root = normalize_optional_text(modelsRoot);
    for endpoint in [&chat_endpoint, &graph_endpoint, &embed_endpoint] {
        validate_provider_request(&to_model_policy(&policy), provider, endpoint, &[])
            .map_err(|violation| violation.message)?;
    }
    let (dto, _errors) = fetch_models_all_endpoints(
        provider,
        &chat_endpoint,
        &graph_endpoint,
        &embed_endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;
    Ok(dto)
}

#[tauri::command]
#[allow(non_snake_case)]
pub(crate) async fn probe_model_provider(
    provider: String,
    chatEndpoint: String,
    graphEndpoint: String,
    embedEndpoint: String,
    apiKey: Option<String>,
    modelsRoot: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    info!(provider = %provider, "probe model provider");
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    let provider = ModelProvider::from_value(&provider);
    let chat_endpoint = normalize_endpoint(provider, &chatEndpoint);
    let graph_endpoint = normalize_endpoint(provider, &graphEndpoint);
    let embed_endpoint = normalize_endpoint(provider, &embedEndpoint);
    let api_key = normalize_optional_text(apiKey);
    let models_root = normalize_optional_text(modelsRoot);
    for endpoint in [&chat_endpoint, &graph_endpoint, &embed_endpoint] {
        validate_provider_request(&to_model_policy(&policy), provider, endpoint, &[])
            .map_err(|violation| violation.message)?;
    }
    let (models, errors) = fetch_models_all_endpoints(
        provider,
        &chat_endpoint,
        &graph_endpoint,
        &embed_endpoint,
        api_key.as_deref(),
        models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;

    let merged = models.merged;
    let mut missing_roles = Vec::new();

    let (chat_model, graph_model, embed_model) = match provider {
        ModelProvider::LlamaCppLocal => (
            settings.local_chat_model.as_deref().unwrap_or(""),
            settings.local_graph_model.as_deref().unwrap_or(""),
            settings.local_embed_model.as_deref().unwrap_or(""),
        ),
        ModelProvider::OpenAiCompatible => (
            settings.remote_chat_model.as_deref().unwrap_or(""),
            settings.remote_graph_model.as_deref().unwrap_or(""),
            settings.remote_embed_model.as_deref().unwrap_or(""),
        ),
    };

    if !chat_model.is_empty() && !model_exists(&merged, chat_model) {
        missing_roles.push("chat".to_string());
    }
    if !graph_model.is_empty() && !model_exists(&merged, graph_model) {
        missing_roles.push("graph".to_string());
    }
    if !embed_model.is_empty() && !model_exists(&merged, embed_model) {
        missing_roles.push("embed".to_string());
    }

    let reachable = errors.is_empty();

    Ok(ModelAvailabilityDto {
        configured: true,
        reachable,
        models: merged,
        missing_roles,
        errors: errors
            .into_iter()
            .map(|err| ModelErrorItem {
                code: err.code,
                message: err.message,
            })
            .collect(),
        checked_provider: Some(provider_to_string(provider)),
        status_code: None,
        status_message: None,
    })
}

#[tauri::command]
pub(crate) async fn validate_model_setup() -> Result<ModelAvailabilityDto, String> {
    let settings = load_app_settings()?;
    let Some(active) = resolve_configured_active_runtime_settings(&settings) else {
        let checked_provider = resolve_explicit_provider(&settings).map(provider_to_string);
        return Ok(ModelAvailabilityDto {
            configured: false,
            reachable: false,
            models: Vec::new(),
            missing_roles: Vec::new(),
            errors: vec![ModelErrorItem {
                code: MODEL_NOT_CONFIGURED_CODE.to_string(),
                message: MODEL_NOT_CONFIGURED_MESSAGE.to_string(),
            }],
            checked_provider,
            status_code: Some(MODEL_NOT_CONFIGURED_CODE.to_string()),
            status_message: Some(MODEL_NOT_CONFIGURED_MESSAGE.to_string()),
        });
    };
    let policy = resolve_enterprise_policy(&settings);
    validate_runtime_model_settings(&to_model_policy(&policy), &to_runtime_model_config(&active))
        .map_err(|violation| violation.message)?;
    let provider = active.provider;
    let (models, endpoint_errors) = fetch_models_all_endpoints(
        provider,
        &active.chat_endpoint,
        &active.graph_endpoint,
        &active.embed_endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await
    .map_err(|err| format!("{}: {}", err.code, err.message))?;

    let merged = models.merged;
    let mut missing_roles = Vec::new();
    let mut errors: Vec<ModelErrorItem> = endpoint_errors
        .into_iter()
        .map(|err| ModelErrorItem {
            code: err.code,
            message: err.message,
        })
        .collect();

    if !model_exists(&merged, &active.chat_model) {
        missing_roles.push("chat".to_string());
    }
    if !model_exists(&merged, &active.graph_model) {
        missing_roles.push("graph".to_string());
    }
    if !model_exists(&merged, &active.embed_model) {
        missing_roles.push("embed".to_string());
    }
    if provider == ModelProvider::OpenAiCompatible
        && !missing_roles.iter().any(|role| role == "embed")
        && let Err(err) = probe_openai_compatible_embedding(
            &active.embed_endpoint,
            active.api_key.as_deref(),
            &active.embed_model,
        )
        .await
    {
        missing_roles.push("embed".to_string());
        errors.push(ModelErrorItem {
            code: err.code,
            message: err.message,
        });
    }

    Ok(ModelAvailabilityDto {
        configured: true,
        reachable: errors.is_empty(),
        models: merged,
        missing_roles,
        errors,
        checked_provider: Some(provider_to_string(provider)),
        status_code: None,
        status_message: None,
    })
}

#[tauri::command]
pub(crate) async fn get_local_model_runtime_status(
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn start_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    start_local_model_role(&role, &profile, &state).await?;
    if role == "embed" {
        resume_engine_indexing_if_ready(&state).await;
    }
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn stop_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    stop_local_model_role(&role, &state).await?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    local_runtime_statuses(&state, &profile).await
}

#[tauri::command]
pub(crate) async fn restart_local_model(
    role: String,
    state: State<'_, DesktopState>,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let role = normalize_local_model_role(&role)?;
    let settings = load_app_settings()?;
    let profile = resolve_model_settings(&settings).local_profile;
    stop_local_model_role(&role, &state).await?;
    start_local_model_role(&role, &profile, &state).await?;
    if role == "embed" {
        resume_engine_indexing_if_ready(&state).await;
    }
    local_runtime_statuses(&state, &profile).await
}

async fn resume_engine_indexing_if_ready(state: &State<'_, DesktopState>) {
    let engine_guard = state.engine.lock().await;
    if let Some(engine) = engine_guard.as_ref() {
        engine.resume_indexing().await;
    }
}

#[tauri::command]
pub(crate) async fn pull_model(
    model: String,
    provider: String,
    endpoint: String,
    api_key: Option<String>,
) -> Result<ModelAvailabilityDto, String> {
    info!(model = %model, provider = %provider, "pull model requested");
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("model name cannot be empty".to_string());
    }
    let provider = ModelProvider::from_value(&provider);
    let endpoint = normalize_endpoint(provider, &endpoint);
    let settings = load_app_settings()?;
    let policy = resolve_enterprise_policy(&settings);
    validate_provider_request(&to_model_policy(&policy), provider, &endpoint, &[])
        .map_err(|violation| violation.message)?;
    let api_key = normalize_optional_text(api_key);
    pull_llama_cpp_model(&endpoint, &model, api_key.as_deref()).await?;
    validate_model_setup().await
}

#[tauri::command]
pub(crate) async fn set_local_models_root(path: String) -> Result<ModelSettingsDto, String> {
    info!(path = %path, "set local models root");
    let mut settings = load_app_settings()?;
    let root = normalize_optional_text(Some(path));
    if let Some(root_path) = root.as_deref() {
        let path = PathBuf::from(root_path);
        if !path.exists() {
            return Err(format!("models root does not exist: {}", path.display()));
        }
        if !path.is_dir() {
            return Err(format!("path is not a directory: {}", path.display()));
        }
        settings.local_models_root = Some(
            path.canonicalize()
                .unwrap_or(path)
                .to_string_lossy()
                .to_string(),
        );
    } else {
        settings.local_models_root = None;
    }
    save_app_settings(&settings)?;
    Ok(resolve_model_settings(&settings))
}

#[tauri::command]
pub(crate) async fn scan_local_model_files(root: Option<String>) -> Result<Vec<String>, String> {
    info!(root = ?root, "scan local model files");
    let root = normalize_optional_text(root);
    if let Some(root) = root {
        return scan_local_model_files_from_root(&PathBuf::from(root));
    }
    let settings = load_app_settings()?;
    let resolved = resolve_model_settings(&settings);
    let Some(root) = resolved.local_profile.models_root else {
        return Ok(Vec::new());
    };
    scan_local_model_files_from_root(&PathBuf::from(root))
}

fn normalize_local_model_role(role: &str) -> Result<String, String> {
    let normalized = role.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "chat" | "graph" | "embed" => Ok(normalized),
        _ => Err(format!("unsupported local model role: {role}")),
    }
}

fn role_endpoint(profile: &LocalModelProfileDto, role: &str) -> String {
    match role {
        "chat" => profile.chat_endpoint.clone(),
        "graph" => profile.graph_endpoint.clone(),
        "embed" => profile.embed_endpoint.clone(),
        _ => String::new(),
    }
}

fn role_model(profile: &LocalModelProfileDto, role: &str) -> String {
    match role {
        "chat" => profile.chat_model.clone(),
        "graph" => profile.graph_model.clone(),
        "embed" => profile.embed_model.clone(),
        _ => String::new(),
    }
}

fn role_model_path(profile: &LocalModelProfileDto, role: &str) -> Option<String> {
    match role {
        "chat" => profile.chat_model_path.clone(),
        "graph" => profile.graph_model_path.clone(),
        "embed" => profile.embed_model_path.clone(),
        _ => None,
    }
}

fn role_context_length(profile: &LocalModelProfileDto, role: &str) -> Option<u32> {
    match role {
        "chat" => profile.chat_context_length,
        "graph" => profile.graph_context_length,
        "embed" => profile.embed_context_length,
        _ => None,
    }
}

fn role_concurrency(profile: &LocalModelProfileDto, role: &str) -> Option<u32> {
    match role {
        "chat" => profile.chat_concurrency,
        "graph" => profile.graph_concurrency,
        "embed" => profile.embed_concurrency,
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
struct LlamaRuntimeArgs {
    n_gpu_layers: Option<i32>,
    batch_size: Option<u32>,
    ubatch_size: Option<u32>,
    threads: Option<u32>,
    threads_batch: Option<u32>,
    flash_attn: bool,
    cache_type_k: Option<String>,
    cache_type_v: Option<String>,
}

fn resolved_llama_runtime_args(profile: &LocalModelProfileDto) -> LlamaRuntimeArgs {
    let preset = profile
        .performance_preset
        .as_deref()
        .unwrap_or("compat")
        .to_ascii_lowercase();
    let mut args = match preset.as_str() {
        "gpu" => LlamaRuntimeArgs {
            n_gpu_layers: Some(-1),
            batch_size: Some(1024),
            ubatch_size: Some(512),
            flash_attn: true,
            ..Default::default()
        },
        "low_vram" | "low-vram" => LlamaRuntimeArgs {
            n_gpu_layers: Some(24),
            batch_size: Some(256),
            ubatch_size: Some(128),
            cache_type_k: Some("q8_0".to_string()),
            cache_type_v: Some("q8_0".to_string()),
            ..Default::default()
        },
        "throughput" => LlamaRuntimeArgs {
            n_gpu_layers: Some(-1),
            batch_size: Some(2048),
            ubatch_size: Some(512),
            flash_attn: true,
            ..Default::default()
        },
        _ => LlamaRuntimeArgs::default(),
    };

    if profile.n_gpu_layers.is_some() {
        args.n_gpu_layers = profile.n_gpu_layers;
    }
    if profile.batch_size.is_some() {
        args.batch_size = profile.batch_size;
    }
    if profile.ubatch_size.is_some() {
        args.ubatch_size = profile.ubatch_size;
    }
    if profile.threads.is_some() {
        args.threads = profile.threads;
    }
    if profile.threads_batch.is_some() {
        args.threads_batch = profile.threads_batch;
    }
    if let Some(flash_attn) = profile.flash_attn {
        args.flash_attn = flash_attn;
    }
    if let Some(cache_type_k) = normalize_optional_text(profile.cache_type_k.clone()) {
        args.cache_type_k = Some(cache_type_k);
    }
    if let Some(cache_type_v) = normalize_optional_text(profile.cache_type_v.clone()) {
        args.cache_type_v = Some(cache_type_v);
    }

    args
}

fn append_llama_runtime_args(command: &mut Command, args: &LlamaRuntimeArgs) {
    if let Some(value) = args.n_gpu_layers {
        command.arg("--n-gpu-layers").arg(value.to_string());
    }
    if let Some(value) = args.batch_size.filter(|value| *value > 0) {
        command.arg("--batch-size").arg(value.to_string());
    }
    if let Some(value) = args.ubatch_size.filter(|value| *value > 0) {
        command.arg("--ubatch-size").arg(value.to_string());
    }
    if let Some(value) = args.threads.filter(|value| *value > 0) {
        command.arg("--threads").arg(value.to_string());
    }
    if let Some(value) = args.threads_batch.filter(|value| *value > 0) {
        command.arg("--threads-batch").arg(value.to_string());
    }
    if args.flash_attn {
        command.arg("--flash-attn");
    }
    if let Some(value) = args
        .cache_type_k
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        command.arg("--cache-type-k").arg(value);
    }
    if let Some(value) = args
        .cache_type_v
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        command.arg("--cache-type-v").arg(value);
    }
}

fn endpoint_port(endpoint: &str) -> Result<u16, String> {
    let parsed = reqwest::Url::parse(endpoint)
        .map_err(|err| format!("invalid endpoint URL ({endpoint}): {err}"))?;
    parsed
        .port_or_known_default()
        .ok_or_else(|| format!("endpoint has no usable port: {endpoint}"))
}

fn resolve_llama_server_path(profile: &LocalModelProfileDto) -> Result<PathBuf, String> {
    if let Some(path) = normalize_optional_text(profile.llama_server_path.clone()) {
        let p = PathBuf::from(path);
        if !p.exists() {
            return Err(format!("llama-server path does not exist: {}", p.display()));
        }
        if !p.is_file() {
            return Err(format!("llama-server path is not a file: {}", p.display()));
        }
        return Ok(p);
    }

    find_executable_on_path("llama-server").ok_or_else(|| {
        "llama-server executable was not found in PATH. Select the llama-server executable in Settings > Models.".to_string()
    })
}

fn find_executable_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let candidates = if cfg!(windows) {
        vec![format!("{name}.exe"), name.to_string()]
    } else {
        vec![name.to_string()]
    };
    for dir in std::env::split_paths(&path_var) {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

async fn local_runtime_statuses(
    state: &State<'_, DesktopState>,
    profile: &LocalModelProfileDto,
) -> Result<LocalModelRuntimeStatusesDto, String> {
    let mut guard = state.local_models.lock().await;
    let roles = ["chat", "graph", "embed"];
    let mut result = Vec::new();
    for role in roles {
        let endpoint = role_endpoint(profile, role);
        let port = endpoint_port(&endpoint).ok();
        let mut remove_dead = false;
        let status = if let Some(process) = guard.get_mut(role) {
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    remove_dead = true;
                    LocalModelRuntimeStatusDto {
                        role: role.to_string(),
                        endpoint,
                        port,
                        pid: None,
                        state: "stopped".to_string(),
                        message: Some(format!("llama-server exited with status {status}")),
                    }
                }
                Ok(None) => LocalModelRuntimeStatusDto {
                    role: role.to_string(),
                    endpoint: process.endpoint.clone(),
                    port: Some(process.port),
                    pid: Some(process.child.id()),
                    state: "running".to_string(),
                    message: Some(format!("{} -> {}", process.model, process.model_path)),
                },
                Err(err) => {
                    remove_dead = true;
                    LocalModelRuntimeStatusDto {
                        role: role.to_string(),
                        endpoint,
                        port,
                        pid: None,
                        state: "error".to_string(),
                        message: Some(format!("failed to read process status: {err}")),
                    }
                }
            }
        } else {
            LocalModelRuntimeStatusDto {
                role: role.to_string(),
                endpoint,
                port,
                pid: None,
                state: "stopped".to_string(),
                message: None,
            }
        };
        if remove_dead {
            guard.remove(role);
        }
        result.push(status);
    }
    Ok(LocalModelRuntimeStatusesDto { roles: result })
}

async fn start_local_model_role(
    role: &str,
    profile: &LocalModelProfileDto,
    state: &State<'_, DesktopState>,
) -> Result<(), String> {
    let endpoint = role_endpoint(profile, role);
    let port = endpoint_port(&endpoint)?;
    let model = role_model(profile, role).trim().to_string();
    if model.is_empty() {
        return Err(format!("{role} model name is empty"));
    }
    let model_path = role_model_path(profile, role)
        .and_then(|value| normalize_optional_text(Some(value)))
        .ok_or_else(|| format!("select a GGUF model file for {role} before starting it"))?;
    let model_path_buf = PathBuf::from(&model_path);
    if !model_path_buf.exists() || !model_path_buf.is_file() {
        return Err(format!(
            "{role} model file does not exist: {}",
            model_path_buf.display()
        ));
    }
    let llama_server = resolve_llama_server_path(profile)?;

    {
        let mut guard = state.local_models.lock().await;
        if let Some(existing) = guard.get_mut(role) {
            if existing
                .child
                .try_wait()
                .map_err(|err| err.to_string())?
                .is_none()
            {
                return Ok(());
            }
            guard.remove(role);
        }
    }

    if !is_port_available(port) {
        let message = format!(
            "port conflict: cannot start {role} model because 127.0.0.1:{port} is already in use"
        );
        error!(role = %role, port = port, endpoint = %endpoint, error = %message, "local llama.cpp port conflict");
        return Err(message);
    }

    let mut command = Command::new(&llama_server);
    command
        .arg("-m")
        .arg(&model_path_buf)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--alias")
        .arg(&model);
    if role == "embed" {
        command.arg("--embedding");
    }
    if let Some(ctx) = role_context_length(profile, role).filter(|value| *value > 0) {
        command.arg("--ctx-size").arg(ctx.to_string());
    }
    if let Some(parallel) = role_concurrency(profile, role).filter(|value| *value > 0) {
        command.arg("--parallel").arg(parallel.to_string());
    }
    let runtime_args = resolved_llama_runtime_args(profile);
    append_llama_runtime_args(&mut command, &runtime_args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command.spawn().map_err(|err| {
        let message = format!(
            "failed to start llama-server for {role}; executable={}; model={}; port={port}; reason={err}",
            llama_server.display(),
            model_path_buf.display()
        );
        error!(role = %role, port = port, model = %model, path = %model_path_buf.display(), error = %err, "local model start failed");
        message
    })?;
    let pid = child.id();
    info!(
        role = %role,
        pid = pid,
        port = port,
        endpoint = %endpoint,
        model = %model,
        model_path = %model_path_buf.display(),
        llama_server = %llama_server.display(),
        "started local llama.cpp model"
    );

    let mut guard = state.local_models.lock().await;
    guard.insert(
        role.to_string(),
        LocalModelProcess {
            child,
            endpoint,
            port,
            model_path: model_path_buf.to_string_lossy().to_string(),
            model,
        },
    );
    Ok(())
}

async fn stop_local_model_role(role: &str, state: &State<'_, DesktopState>) -> Result<(), String> {
    let process = {
        let mut guard = state.local_models.lock().await;
        guard.remove(role)
    };
    let Some(mut process) = process else {
        return Ok(());
    };
    let pid = process.child.id();
    match process.child.kill() {
        Ok(()) => {
            let _ = process.child.wait();
            info!(role = %role, pid = pid, port = process.port, "stopped local llama.cpp model");
            Ok(())
        }
        Err(err) => {
            error!(role = %role, pid = pid, error = %err, "failed to stop local llama.cpp model");
            Err(format!(
                "failed to stop {role} llama-server process {pid}: {err}"
            ))
        }
    }
}
