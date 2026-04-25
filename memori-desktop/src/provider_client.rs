use crate::*;

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

pub(crate) async fn fetch_models_all_endpoints(
    provider: ModelProvider,
    chat_endpoint: &str,
    graph_endpoint: &str,
    embed_endpoint: &str,
    api_key: Option<&str>,
    models_root: Option<&str>,
) -> Result<(ProviderModelsDto, Vec<ProviderModelFetchError>), ProviderModelFetchError> {
    let mut errors = Vec::new();

    match provider {
        ModelProvider::OllamaLocal => {
            let from_folder = models_root
                .map(PathBuf::from)
                .map(|root| scan_local_model_files_from_root(&root))
                .transpose()
                .map_err(|err| ProviderModelFetchError {
                    code: "models_root_invalid".to_string(),
                    message: err,
                })?
                .unwrap_or_default();

            let mut from_service = Vec::new();
            for endpoint in [chat_endpoint, graph_endpoint, embed_endpoint] {
                match list_ollama_models(endpoint).await {
                    Ok(models) => from_service.extend(models),
                    Err(err) => errors.push(err),
                }
            }

            let mut service_set = BTreeSet::new();
            for m in from_service {
                service_set.insert(m);
            }
            let from_service_deduped: Vec<String> = service_set.into_iter().collect();
            Ok((merge_model_candidates(from_folder, from_service_deduped), errors))
        }
        ModelProvider::OpenAiCompatible => {
            let mut from_service = Vec::new();
            for endpoint in [chat_endpoint, graph_endpoint, embed_endpoint] {
                match list_openai_compatible_models(endpoint, api_key).await {
                    Ok(models) => from_service.extend(models),
                    Err(err) => errors.push(err),
                }
            }

            let mut service_set = BTreeSet::new();
            for m in from_service {
                service_set.insert(m);
            }
            let from_service_deduped: Vec<String> = service_set.into_iter().collect();
            Ok((merge_model_candidates(Vec::new(), from_service_deduped), errors))
        }
    }
}

pub(crate) async fn list_ollama_models(
    endpoint: &str,
) -> Result<Vec<String>, ProviderModelFetchError> {
    #[derive(Debug, Deserialize)]
    struct OllamaTagResp {
        models: Vec<OllamaTagItem>,
    }
    #[derive(Debug, Deserialize)]
    struct OllamaTagItem {
        name: String,
    }

    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new().get(url).send(),
    )
    .await
    .map_err(|_| ProviderModelFetchError {
        code: "request_timeout".to_string(),
        message: format!("连接 Ollama 超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接 Ollama 失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("Ollama 模型列表请求失败: status={}, body={body}", status),
        });
    }
    let parsed: OllamaTagResp = response
        .json()
        .await
        .map_err(|err| ProviderModelFetchError {
            code: "endpoint_unreachable".to_string(),
            message: format!("解析 Ollama 模型列表失败: {err}"),
        })?;
    Ok(parsed.models.into_iter().map(|m| m.name).collect())
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
        message: format!("连接远程模型服务超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "endpoint_unreachable".to_string(),
        message: format!("连接远程模型服务失败: {err}"),
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
                message: format!("解析远程模型列表失败: {err}"),
            })?;
    Ok(parsed.data.into_iter().map(|m| m.id).collect())
}

pub(crate) async fn probe_openai_compatible_embedding(
    endpoint: &str,
    api_key: Option<&str>,
    model: &str,
) -> Result<(), ProviderModelFetchError> {
    #[derive(Debug, Serialize)]
    struct OpenAiEmbeddingProbeRequest<'a> {
        model: &'a str,
        input: &'a str,
    }

    #[derive(Debug, Deserialize)]
    struct OpenAiEmbeddingProbeResponse {
        data: Vec<OpenAiEmbeddingProbeItem>,
    }

    #[derive(Debug, Deserialize)]
    struct OpenAiEmbeddingProbeItem {
        embedding: Vec<f32>,
    }

    let url = format!("{}/v1/embeddings", endpoint.trim_end_matches('/'));
    let mut request = reqwest::Client::new()
        .post(url)
        .json(&OpenAiEmbeddingProbeRequest {
            model,
            input: "ping",
        });
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
        message: format!("向量模型探测超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS),
    })?
    .map_err(|err| ProviderModelFetchError {
        code: "embed_probe_failed".to_string(),
        message: format!("向量模型探测失败: {err}"),
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderModelFetchError {
            code: "embed_probe_failed".to_string(),
            message: format!("status={}, body={body}", status),
        });
    }
    let parsed: OpenAiEmbeddingProbeResponse =
        response
            .json()
            .await
            .map_err(|err| ProviderModelFetchError {
                code: "embed_probe_failed".to_string(),
                message: format!("解析向量模型探测响应失败: {err}"),
            })?;
    let ok = parsed
        .data
        .first()
        .map(|item| !item.embedding.is_empty())
        .unwrap_or(false);
    if !ok {
        return Err(ProviderModelFetchError {
            code: "embed_probe_failed".to_string(),
            message: "向量模型未返回有效 embedding".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn scan_local_model_files_from_root(root: &Path) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Err(format!("模型目录不存在: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("路径不是目录: {}", root.display()));
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
    let entries =
        fs::read_dir(dir).map_err(|err| format!("读取模型目录失败({}): {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("读取模型目录项失败({}): {err}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|err| format!("读取模型目录元数据失败({}): {err}", path.display()))?;
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

pub(crate) async fn pull_ollama_model(
    endpoint: &str,
    model: &str,
    _api_key: Option<&str>,
) -> Result<(), String> {
    #[derive(Debug, Serialize)]
    struct PullBody<'a> {
        name: &'a str,
        stream: bool,
    }
    let url = format!("{}/api/pull", endpoint.trim_end_matches('/'));
    let response = timeout(
        Duration::from_secs(PROVIDER_HTTP_TIMEOUT_SECS),
        reqwest::Client::new()
            .post(url)
            .json(&PullBody {
                name: model,
                stream: false,
            })
            .send(),
    )
    .await
    .map_err(|_| format!("拉取模型超时({}s)", PROVIDER_HTTP_TIMEOUT_SECS))?
    .map_err(|err| format!("拉取模型失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("拉取模型失败: status={}, body={body}", status));
    }
    Ok(())
}

pub(crate) fn model_exists(models: &[String], expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() {
        return false;
    }
    models.iter().any(|m| m == expected)
        || (!expected.contains(':') && models.iter().any(|m| m == &format!("{expected}:latest")))
}
