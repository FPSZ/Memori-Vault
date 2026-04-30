use crate::*;

pub(crate) async fn fetch_provider_models(
    provider: ModelProvider,
    endpoint: &str,
    api_key: Option<&str>,
    models_root: Option<&str>,
) -> Result<ProviderModelsDto, ProviderModelFetchError> {
    match provider {
        ModelProvider::LlamaCppLocal => {
            let from_folder = models_root
                .map(PathBuf::from)
                .map(|root| scan_local_model_files_from_root(&root))
                .transpose()
                .map_err(|err| ProviderModelFetchError {
                    code: "models_root_invalid".to_string(),
                    message: err,
                })?
                .unwrap_or_default();
            let from_service = list_openai_compatible_models(endpoint, None).await?;
            Ok(merge_model_candidates(from_folder, from_service))
        }
        ModelProvider::OpenAiCompatible => {
            let from_service = list_openai_compatible_models(endpoint, api_key).await?;
            Ok(merge_model_candidates(Vec::new(), from_service))
        }
    }
}

pub(crate) fn merge_model_candidates(
    from_folder: Vec<String>,
    from_service: Vec<String>,
) -> ProviderModelsDto {
    let mut merged_set = BTreeSet::new();
    for model in &from_folder {
        merged_set.insert(model.clone());
    }
    for model in &from_service {
        merged_set.insert(model.clone());
    }
    ProviderModelsDto {
        from_folder,
        from_service,
        merged: merged_set.into_iter().collect(),
    }
}

pub(crate) async fn list_openai_compatible_models(
    endpoint: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OpenAiModelsResp {
        data: Vec<OpenAiModelItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OpenAiModelItem {
        id: String,
    }
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let mut request = reqwest::Client::new().get(url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        request.send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!(
            "model service request timed out ({}s)",
            PROVIDER_HTTP_TIMEOUT_SECS
        ),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("model service endpoint unreachable: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let code = if status.as_u16() == 401 || status.as_u16() == 403 {
            "auth_failed"
        } else {
            "endpoint_unreachable"
        };
        return Err(ProviderModelFetchError {
            code: code.to_string(),
            message: format!("status={}, body={body}", status),
        });
    }
    let parsed: OpenAiModelsResp =
        response
            .json()
            .await
            .map_err(|err| ProviderModelFetchError {
                code: "endpoint_unreachable".to_string(),
                message: format!("failed to parse model list response: {err}"),
            })?;
    Ok(parsed.data.into_iter().map(|m| m.id).collect())
}

pub(crate) fn scan_local_model_files_from_root(root: &Path) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Err(format!("models root does not exist: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("path is not a directory: {}", root.display()));
    }
    let mut set = BTreeSet::new();
    collect_local_model_files_recursive(root, &mut set, 0, 8)?;
    Ok(set.into_iter().collect())
}

pub(crate) fn collect_local_model_files_recursive(
    dir: &Path,
    set: &mut BTreeSet<String>,
    depth: usize,
    max_depth: usize,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read model directory ({}): {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read model directory entry ({}): {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(|err| {
            format!(
                "failed to read model file metadata ({}): {err}",
                path.display()
            )
        })?;
        if metadata.is_dir() {
            collect_local_model_files_recursive(&path, set, depth + 1, max_depth)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("gguf") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|v| v.to_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                set.insert(trimmed.to_string());
            }
        }
    }
    Ok(())
}

pub(crate) async fn pull_llama_cpp_model(
    _endpoint: &str,
    _model: &str,
    _api_key: Option<&str>,
) -> Result<(), String> {
    Err("llama.cpp local runtime does not support pulling models. Place GGUF files in the configured models root and start llama-server manually.".to_string())
}

pub(crate) fn model_exists(models: &[String], expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    models.iter().any(|m| m == expected)
        || (!expected.contains(':') && models.iter().any(|m| m == &format!("{expected}:latest")))
}

pub(crate) fn resolve_enterprise_policy(settings: &AppSettings) -> EnterprisePolicyDto {
    let indexing = resolve_indexing_config(settings);
    let allowed_model_endpoints = settings
        .enterprise_allowed_model_endpoints
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| normalize_policy_endpoint(&item))
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let allowed_models = settings
        .enterprise_allowed_models
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    EnterprisePolicyDto {
        egress_mode: settings
            .enterprise_egress_mode
            .as_deref()
            .map(EgressMode::from_value)
            .unwrap_or_default(),
        allowed_model_endpoints,
        allowed_models,
        indexing_default_mode: indexing.mode.as_str().to_string(),
        resource_budget_default: indexing.resource_budget.as_str().to_string(),
        auth: AuthConfigDto {
            issuer: settings
                .oidc_issuer
                .clone()
                .unwrap_or_else(|| "https://example-idp.local".to_string()),
            client_id: settings
                .oidc_client_id
                .clone()
                .unwrap_or_else(|| "memori-vault-enterprise".to_string()),
            redirect_uri: settings
                .oidc_redirect_uri
                .clone()
                .unwrap_or_else(|| "http://localhost:3757/api/auth/oidc/login".to_string()),
            roles_claim: settings
                .oidc_roles_claim
                .clone()
                .unwrap_or_else(|| "roles".to_string()),
        },
    }
}
