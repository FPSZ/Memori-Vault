use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::protocol::*;
use super::{engine_from_state, normalize_mcp_top_k, parse_params};
use crate::*;

const TOOL_ASK: &str = "ask";
const TOOL_SEARCH: &str = "search";
const TOOL_GET_SOURCE: &str = "get_source";
const TOOL_OPEN_SOURCE: &str = "open_source";
const TOOL_GET_VAULT_STATS: &str = "get_vault_stats";
const TOOL_GET_INDEXING_STATUS: &str = "get_indexing_status";
const TOOL_TRIGGER_REINDEX: &str = "trigger_reindex";
const TOOL_PAUSE_INDEXING: &str = "pause_indexing";
const TOOL_RESUME_INDEXING: &str = "resume_indexing";
const TOOL_SET_INDEXING_MODE: &str = "set_indexing_mode";
const TOOL_SET_WATCH_ROOT: &str = "set_watch_root";
const TOOL_GET_MODEL_SETTINGS: &str = "get_model_settings";
const TOOL_SET_MODEL_SETTINGS: &str = "set_model_settings";
const TOOL_VALIDATE_MODEL_SETUP: &str = "validate_model_setup";
const TOOL_LIST_PROVIDER_MODELS: &str = "list_provider_models";
const TOOL_PROBE_MODEL_PROVIDER: &str = "probe_model_provider";
const TOOL_PULL_MODEL: &str = "pull_model";
const TOOL_GET_GRAPH_CONTEXT: &str = "get_graph_context";
const TOOL_GET_GRAPH_NEIGHBORS: &str = "get_graph_neighbors";
const TOOL_LIST_GRAPH_ENTITIES: &str = "list_graph_entities";
const TOOL_GET_APP_SETTINGS: &str = "get_app_settings";
const TOOL_GET_ENTERPRISE_POLICY: &str = "get_enterprise_policy";
const TOOL_GET_RUNTIME_BASELINE: &str = "get_runtime_baseline";
const TOOL_RANK_SETTINGS: &str = "rank_settings";
const TOOL_MEMORY_SEARCH: &str = "memory_search";
const TOOL_MEMORY_ADD: &str = "memory_add";
const TOOL_MEMORY_UPDATE: &str = "memory_update";
const TOOL_MEMORY_LIST_RECENT: &str = "memory_list_recent";
const TOOL_MEMORY_GET_SOURCE: &str = "memory_get_source";

#[derive(Debug, Deserialize)]
struct AskArgs {
    query: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default, alias = "topK")]
    top_k: Option<usize>,
    #[serde(default, alias = "scopePaths")]
    scope_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SearchArgs {
    query: String,
    #[serde(default, alias = "topK")]
    top_k: Option<usize>,
    #[serde(default, alias = "scopePaths")]
    scope_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct SourceArgs {
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    chunk_id: Option<i64>,
    #[serde(default)]
    citation_index: Option<usize>,
    #[serde(default)]
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenSourceArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
struct GraphContextArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    chunk_ids: Vec<i64>,
    #[serde(default, alias = "topK")]
    top_k: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GraphNeighborsArgs {
    entity_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GraphEntitiesArgs {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MemorySearchArgs {
    query: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MemoryAddArgs {
    scope: String,
    #[serde(default)]
    scope_id: Option<String>,
    #[serde(default)]
    layer: Option<String>,
    memory_type: String,
    #[serde(default)]
    title: Option<String>,
    content: String,
    #[serde(default)]
    source_type: Option<String>,
    source_ref: String,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    links: Vec<String>,
    #[serde(default)]
    supersedes: Option<i64>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryUpdateArgs {
    memory_id: i64,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    supersedes: Option<i64>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryListRecentArgs {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct MemoryGetSourceArgs {
    memory_id: i64,
}

pub fn list_tools() -> ListToolsResult {
    ListToolsResult {
        tools: vec![
            tool(
                TOOL_ASK,
                "Ask Memori-Vault and return answer, citations, evidence, and retrieval metrics.",
                schema(&["query"]),
            ),
            tool(
                TOOL_SEARCH,
                "Retrieve ranked source chunks without generating an answer.",
                schema(&["query"]),
            ),
            tool(
                TOOL_GET_SOURCE,
                "Read a source by file_path, chunk_id, or citation_index with a query.",
                schema(&[]),
            ),
            tool(
                TOOL_OPEN_SOURCE,
                "Open a local source file in the desktop environment.",
                schema(&["path"]),
            ),
            tool(
                TOOL_GET_VAULT_STATS,
                "Return vault document/chunk/graph statistics.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_INDEXING_STATUS,
                "Return indexing phase, queue, and rebuild status.",
                schema(&[]),
            ),
            tool(
                TOOL_TRIGGER_REINDEX,
                "Trigger a full reindex task.",
                schema(&[]),
            ),
            tool(
                TOOL_PAUSE_INDEXING,
                "Pause background indexing.",
                schema(&[]),
            ),
            tool(
                TOOL_RESUME_INDEXING,
                "Resume background indexing.",
                schema(&[]),
            ),
            tool(
                TOOL_SET_INDEXING_MODE,
                "Set indexing mode/resource budget/schedule.",
                schema(&["indexing_mode", "resource_budget"]),
            ),
            tool(
                TOOL_SET_WATCH_ROOT,
                "Change the vault watch root and reload the engine.",
                schema(&["path"]),
            ),
            tool(
                TOOL_GET_MODEL_SETTINGS,
                "Return active local/remote model settings.",
                schema(&[]),
            ),
            tool(
                TOOL_SET_MODEL_SETTINGS,
                "Replace model settings and hot-reload runtime.",
                schema(&["active_provider", "local_profile", "remote_profile"]),
            ),
            tool(
                TOOL_VALIDATE_MODEL_SETUP,
                "Validate active model configuration.",
                schema(&[]),
            ),
            tool(
                TOOL_LIST_PROVIDER_MODELS,
                "List model candidates from provider/folder.",
                schema(&["provider", "endpoint"]),
            ),
            tool(
                TOOL_PROBE_MODEL_PROVIDER,
                "Probe a model provider endpoint.",
                schema(&["provider", "endpoint"]),
            ),
            tool(
                TOOL_PULL_MODEL,
                "Pull a local Ollama model; may be long-running.",
                schema(&["model", "provider", "endpoint"]),
            ),
            tool(
                TOOL_GET_GRAPH_CONTEXT,
                "Return graph context for a query or chunk ids.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_GRAPH_NEIGHBORS,
                "Return 1-hop graph neighbors and source chunks for an entity id.",
                schema(&["entity_id"]),
            ),
            tool(
                TOOL_LIST_GRAPH_ENTITIES,
                "Search graph entities by id/name/label/description.",
                schema(&["query"]),
            ),
            tool(
                TOOL_GET_APP_SETTINGS,
                "Return app settings such as watch root and indexing config.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_ENTERPRISE_POLICY,
                "Return enterprise egress/model policy.",
                schema(&[]),
            ),
            tool(
                TOOL_GET_RUNTIME_BASELINE,
                "Return runtime retrieval baseline diagnostics.",
                schema(&[]),
            ),
            tool(
                TOOL_RANK_SETTINGS,
                "Rank settings tabs for a natural-language settings query.",
                schema(&["query", "candidates"]),
            ),
            tool(
                TOOL_MEMORY_SEARCH,
                "Search STM/MTM/LTM memories without mixing them into document citations.",
                schema(&["query"]),
            ),
            tool(
                TOOL_MEMORY_ADD,
                "Add an audited long-term memory note/fact with explicit scope and source_ref.",
                schema(&["scope", "memory_type", "content", "source_ref"]),
            ),
            tool(
                TOOL_MEMORY_UPDATE,
                "Update or supersede an existing memory and write lifecycle audit log.",
                schema(&["memory_id"]),
            ),
            tool(
                TOOL_MEMORY_LIST_RECENT,
                "List recent memories for a scope.",
                schema(&[]),
            ),
            tool(
                TOOL_MEMORY_GET_SOURCE,
                "Read a memory and its lifecycle logs/source pointer.",
                schema(&["memory_id"]),
            ),
        ],
    }
}

pub async fn call_tool(
    state: ServerState,
    params: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let call = parse_params::<CallToolParams>(params)?;
    let result = match call.name.as_str() {
        TOOL_ASK => ask(state, call.arguments).await,
        TOOL_SEARCH => search(state, call.arguments).await,
        TOOL_GET_SOURCE => get_source(state, call.arguments).await,
        TOOL_OPEN_SOURCE => open_source(call.arguments).await,
        TOOL_GET_VAULT_STATS => get_vault_stats(state).await,
        TOOL_GET_INDEXING_STATUS => get_indexing_status(state).await,
        TOOL_TRIGGER_REINDEX => trigger_reindex(state).await,
        TOOL_PAUSE_INDEXING => pause_indexing(state).await,
        TOOL_RESUME_INDEXING => resume_indexing(state).await,
        TOOL_SET_INDEXING_MODE => set_indexing_mode(state, call.arguments).await,
        TOOL_SET_WATCH_ROOT => set_watch_root(state, call.arguments).await,
        TOOL_GET_MODEL_SETTINGS => Ok(json!(resolve_model_settings(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        ))),
        TOOL_SET_MODEL_SETTINGS => set_model_settings(state, call.arguments).await,
        TOOL_VALIDATE_MODEL_SETUP => validate_model_setup().await,
        TOOL_LIST_PROVIDER_MODELS => list_provider_models(call.arguments).await,
        TOOL_PROBE_MODEL_PROVIDER => probe_model_provider(call.arguments).await,
        TOOL_PULL_MODEL => pull_model(call.arguments).await,
        TOOL_GET_GRAPH_CONTEXT => get_graph_context(state, call.arguments).await,
        TOOL_GET_GRAPH_NEIGHBORS => get_graph_neighbors(state, call.arguments).await,
        TOOL_LIST_GRAPH_ENTITIES => list_graph_entities(state, call.arguments).await,
        TOOL_GET_APP_SETTINGS => get_app_settings_value(),
        TOOL_GET_ENTERPRISE_POLICY => Ok(json!(resolve_enterprise_policy(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        ))),
        TOOL_GET_RUNTIME_BASELINE => get_runtime_baseline(state).await,
        TOOL_RANK_SETTINGS => rank_settings(state, call.arguments).await,
        TOOL_MEMORY_SEARCH => memory_search(state, call.arguments).await,
        TOOL_MEMORY_ADD => memory_add(state, call.arguments).await,
        TOOL_MEMORY_UPDATE => memory_update(state, call.arguments).await,
        TOOL_MEMORY_LIST_RECENT => memory_list_recent(state, call.arguments).await,
        TOOL_MEMORY_GET_SOURCE => memory_get_source(state, call.arguments).await,
        _ => Err(JsonRpcError::method_not_found(format!(
            "unknown tool: {}",
            call.name
        ))),
    };

    match result {
        Ok(value) => to_tool_result(value, false),
        Err(error) => to_tool_result(json!({ "error": error.message }), true),
    }
}

async fn ask(state: ServerState, args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<AskArgs>(args)?;
    if args.query.trim().is_empty() {
        return Err(JsonRpcError::invalid_params("query is required"));
    }
    let engine = engine_from_state(&state).await?;
    let scope_paths = normalize_scope_paths(args.scope_paths);
    let response = engine
        .ask_structured(
            &args.query,
            args.lang.as_deref(),
            optional_scope_refs(&scope_paths),
            args.top_k,
        )
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(response))
}

async fn search(state: ServerState, args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<SearchArgs>(args)?;
    if args.query.trim().is_empty() {
        return Err(JsonRpcError::invalid_params("query is required"));
    }
    let engine = engine_from_state(&state).await?;
    let scope_paths = normalize_scope_paths(args.scope_paths);
    let inspection = engine
        .retrieve_structured(
            &args.query,
            optional_scope_refs(&scope_paths),
            Some(normalize_mcp_top_k(args.top_k, 10)),
        )
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(inspection))
}

async fn get_source(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<SourceArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    if let Some(chunk_id) = args.chunk_id {
        let store = engine.state().vector_store.clone();
        let Some(chunk) = store
            .get_chunk_by_id(chunk_id)
            .await
            .map_err(|err| JsonRpcError::internal_error(err.to_string()))?
        else {
            return Err(JsonRpcError::invalid_params(format!(
                "chunk not found: {chunk_id}"
            )));
        };
        let doc = store
            .get_document_by_id(chunk.doc_id)
            .await
            .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
        return Ok(json!({ "chunk": chunk, "document": doc }));
    }

    if let Some(index) = args.citation_index {
        let query = args.query.as_deref().unwrap_or_default();
        if query.trim().is_empty() {
            return Err(JsonRpcError::invalid_params(
                "query is required when using citation_index",
            ));
        }
        let inspection = engine
            .retrieve_structured(query, None, Some(index.max(1)))
            .await
            .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
        let Some(citation) = inspection
            .citations
            .into_iter()
            .find(|item| item.index == index)
        else {
            return Err(JsonRpcError::invalid_params(format!(
                "citation not found: {index}"
            )));
        };
        return Ok(json!(citation));
    }

    if let Some(path) = args.file_path {
        let store = engine.state().vector_store.clone();
        let doc = store
            .get_document_by_file_path(Path::new(&path))
            .await
            .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
        let chunks = match doc.as_ref() {
            Some(doc) => store
                .get_chunks_by_doc_id(doc.id)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?,
            None => Vec::new(),
        };
        return Ok(json!({ "document": doc, "chunks": chunks }));
    }

    Err(JsonRpcError::invalid_params(
        "one of file_path, chunk_id, or citation_index is required",
    ))
}

async fn open_source(args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<OpenSourceArgs>(args)?;
    open_source_path(&args.path).map_err(JsonRpcError::internal_error)?;
    Ok(json!({ "ok": true, "path": args.path }))
}

async fn get_vault_stats(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    let stats = engine
        .get_vault_stats()
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(stats))
}

async fn get_indexing_status(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    let status = engine
        .get_indexing_status()
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(status))
}

async fn trigger_reindex(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    engine
        .trigger_reindex()
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "task_id": format!("reindex-{}", chrono_like_now_token()) }))
}

async fn pause_indexing(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    engine.pause_indexing().await;
    Ok(json!({ "ok": true }))
}

async fn resume_indexing(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    engine.resume_indexing().await;
    Ok(json!({ "ok": true }))
}

async fn set_indexing_mode(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<SetIndexingModePayload>(args)?;
    let mut settings = load_app_settings().map_err(JsonRpcError::internal_error)?;
    let mode = IndexingMode::from_value(&payload.indexing_mode);
    let budget = ResourceBudget::from_value(&payload.resource_budget);
    let schedule_window = if mode == IndexingMode::Scheduled {
        Some(ScheduleWindow {
            start: payload
                .schedule_start
                .unwrap_or_else(|| "00:00".to_string()),
            end: payload.schedule_end.unwrap_or_else(|| "06:00".to_string()),
        })
    } else {
        None
    };
    settings.indexing_mode = Some(mode.as_str().to_string());
    settings.resource_budget = Some(budget.as_str().to_string());
    settings.schedule_start = schedule_window.as_ref().map(|window| window.start.clone());
    settings.schedule_end = schedule_window.as_ref().map(|window| window.end.clone());
    save_app_settings(&settings).map_err(JsonRpcError::internal_error)?;
    let engine = engine_from_state(&state).await?;
    engine
        .set_indexing_config(IndexingConfig {
            mode,
            resource_budget: budget,
            schedule_window,
        })
        .await;
    get_app_settings_value()
}

async fn set_watch_root(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<SetWatchRootRequest>(args)?;
    let trimmed = payload.path.trim();
    if trimmed.is_empty() {
        return Err(JsonRpcError::invalid_params("path is required"));
    }
    let watch_root = PathBuf::from(trimmed);
    if !watch_root.is_dir() {
        return Err(JsonRpcError::invalid_params(format!(
            "not a directory: {}",
            watch_root.display()
        )));
    }
    let canonical = watch_root
        .canonicalize()
        .map_err(|err| JsonRpcError::invalid_params(format!("canonicalize failed: {err}")))?;
    let mut settings = load_app_settings().map_err(JsonRpcError::internal_error)?;
    settings.watch_root = Some(canonical.to_string_lossy().to_string());
    save_app_settings(&settings).map_err(JsonRpcError::internal_error)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        canonical,
        "mcp_set_watch_root",
    )
    .await
    .map_err(JsonRpcError::internal_error)?;
    get_app_settings_value()
}

async fn set_model_settings(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<ModelSettingsDto>(args)?;
    let mut settings = load_app_settings().map_err(JsonRpcError::internal_error)?;
    let normalized =
        normalize_model_settings_payload(payload).map_err(JsonRpcError::invalid_params)?;
    let policy = resolve_enterprise_policy(&settings);
    let active_runtime = resolve_active_runtime_settings(&normalized);
    validate_runtime_model_settings(
        &to_model_policy(&policy),
        &to_runtime_model_config(&active_runtime),
    )
    .map_err(|err| JsonRpcError::invalid_params(err.message))?;
    settings.active_provider = Some(normalized.active_provider.clone());
    settings.local_endpoint = Some(normalized.local_profile.endpoint.clone());
    settings.local_models_root = normalized.local_profile.models_root.clone();
    settings.local_chat_model = Some(normalized.local_profile.chat_model.clone());
    settings.local_graph_model = Some(normalized.local_profile.graph_model.clone());
    settings.local_embed_model = Some(normalized.local_profile.embed_model.clone());
    settings.remote_endpoint = Some(normalized.remote_profile.endpoint.clone());
    settings.remote_api_key = normalized.remote_profile.api_key.clone();
    settings.remote_chat_model = Some(normalized.remote_profile.chat_model.clone());
    settings.remote_graph_model = Some(normalized.remote_profile.graph_model.clone());
    settings.remote_embed_model = Some(normalized.remote_profile.embed_model.clone());
    save_app_settings(&settings).map_err(JsonRpcError::internal_error)?;
    apply_model_settings_to_env(active_runtime);
    let watch_root =
        resolve_watch_root_from_settings(&settings).map_err(JsonRpcError::internal_error)?;
    replace_engine(
        &state.engine,
        &state.init_error,
        watch_root,
        "mcp_set_model_settings",
    )
    .await
    .map_err(JsonRpcError::internal_error)?;
    Ok(json!(normalized))
}

async fn validate_model_setup() -> Result<JsonValue, JsonRpcError> {
    let settings = load_app_settings().map_err(JsonRpcError::internal_error)?;
    let model_settings = resolve_model_settings(&settings);
    let active = resolve_active_runtime_settings(&model_settings);
    let models = fetch_provider_models(
        active.provider,
        &active.chat_endpoint,
        active.api_key.as_deref(),
        active.models_root.as_deref(),
    )
    .await
    .map_err(|err| JsonRpcError::internal_error(format!("{}: {}", err.code, err.message)))?;
    let mut missing_roles = Vec::new();
    if !model_exists(&models.merged, &active.chat_model) {
        missing_roles.push("chat".to_string());
    }
    if !model_exists(&models.merged, &active.graph_model) {
        missing_roles.push("graph".to_string());
    }
    if !model_exists(&models.merged, &active.embed_model) {
        missing_roles.push("embed".to_string());
    }
    Ok(json!({
        "reachable": true,
        "models": models.merged,
        "missing_roles": missing_roles,
        "checked_provider": provider_to_string(active.provider)
    }))
}

async fn list_provider_models(args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<ListProviderModelsRequest>(args)?;
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    let models = fetch_provider_models(
        provider,
        &endpoint,
        normalize_optional_text(payload.api_key).as_deref(),
        normalize_optional_text(payload.models_root).as_deref(),
    )
    .await
    .map_err(|err| JsonRpcError::internal_error(format!("{}: {}", err.code, err.message)))?;
    Ok(json!(models))
}

async fn probe_model_provider(args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<ProbeProviderRequest>(args)?;
    let provider = ModelProvider::from_value(&payload.provider);
    let endpoint = normalize_endpoint(provider, &payload.endpoint);
    match fetch_provider_models(
        provider,
        &endpoint,
        normalize_optional_text(payload.api_key).as_deref(),
        normalize_optional_text(payload.models_root).as_deref(),
    )
    .await
    {
        Ok(models) => Ok(
            json!({ "reachable": true, "models": models.merged, "errors": [], "checked_provider": provider_to_string(provider) }),
        ),
        Err(err) => Ok(
            json!({ "reachable": false, "models": [], "errors": [{ "code": err.code, "message": err.message }], "checked_provider": provider_to_string(provider) }),
        ),
    }
}

async fn pull_model(args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<PullModelRequest>(args)?;
    if ModelProvider::from_value(&payload.provider) != ModelProvider::OllamaLocal {
        return Err(JsonRpcError::invalid_params(
            "pull_model only supports ollama_local",
        ));
    }
    let endpoint = normalize_endpoint(ModelProvider::OllamaLocal, &payload.endpoint);
    pull_ollama_model(&endpoint, &payload.model, None)
        .await
        .map_err(JsonRpcError::internal_error)?;
    Ok(json!({ "ok": true, "model": payload.model }))
}

async fn get_graph_context(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<GraphContextArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let chunk_ids = if args.chunk_ids.is_empty() {
        let query = args.query.as_deref().unwrap_or_default();
        if query.trim().is_empty() {
            return Err(JsonRpcError::invalid_params(
                "query or chunk_ids is required",
            ));
        }
        let inspection = engine
            .retrieve_structured(query, None, Some(normalize_mcp_top_k(args.top_k, 8)))
            .await
            .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
        let store = engine.state().vector_store.clone();
        let mut ids = Vec::new();
        for evidence in inspection.evidence {
            if let Some(id) = store
                .resolve_chunk_id(Path::new(&evidence.file_path), evidence.chunk_index)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?
            {
                ids.push(id);
            }
        }
        ids
    } else {
        args.chunk_ids
    };
    let context = engine
        .state()
        .vector_store
        .get_graph_context_for_chunks(&chunk_ids)
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "chunk_ids": chunk_ids, "context": context }))
}

async fn get_graph_neighbors(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<GraphNeighborsArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let result = engine
        .state()
        .vector_store
        .get_graph_neighbors(&args.entity_id, args.limit.unwrap_or(30))
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(result))
}

async fn list_graph_entities(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<GraphEntitiesArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let result = engine
        .state()
        .vector_store
        .search_graph_nodes(&args.query, args.limit.unwrap_or(20))
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "nodes": result }))
}

async fn get_runtime_baseline(state: ServerState) -> Result<JsonValue, JsonRpcError> {
    let engine = engine_from_state(&state).await?;
    let baseline = engine
        .get_runtime_retrieval_baseline()
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!(baseline))
}

async fn rank_settings(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let payload = parse_params::<RankSettingsRequest>(args)?;
    let query = payload.query.trim();
    if query.is_empty() || payload.candidates.is_empty() {
        return Ok(json!({ "keys": [] }));
    }
    let engine = engine_from_state(&state).await?;
    let candidate_lines = payload
        .candidates
        .iter()
        .map(|item| format!("{} => {}", item.key.trim(), item.text.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "You are a settings retrieval assistant.\nQuery: {query}\nCandidates:\n{candidate_lines}\n\nReturn only a JSON array of best-matching keys, max 3. Do not output explanations."
    );
    let answer = engine
        .generate_answer(&prompt, "", "")
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    let keys = serde_json::from_str::<Vec<String>>(&answer).unwrap_or_default();
    Ok(json!({ "keys": keys }))
}

async fn memory_search(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<MemorySearchArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let scope = parse_memory_scope(args.scope.as_deref())?;
    let layer = parse_memory_layer(args.layer.as_deref())?;
    let memories = engine
        .state()
        .vector_store
        .search_memories(memori_core::MemorySearchOptions {
            query: args.query,
            scope,
            layer,
            limit: args.limit.unwrap_or(20),
        })
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "memories": memories }))
}

async fn memory_add(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<MemoryAddArgs>(args)?;
    if args.content.trim().is_empty() {
        return Err(JsonRpcError::invalid_params("content is required"));
    }
    if args.source_ref.trim().is_empty() {
        return Err(JsonRpcError::invalid_params(
            "source_ref is required for audited memory writes",
        ));
    }
    let engine = engine_from_state(&state).await?;
    let scope = parse_memory_scope(Some(&args.scope))?.unwrap_or_default();
    let layer = parse_memory_layer(args.layer.as_deref())?.unwrap_or(memori_core::MemoryLayer::Mtm);
    let source_type = parse_memory_source_type(args.source_type.as_deref())?
        .unwrap_or(memori_core::MemorySourceType::ConversationTurn);
    let status =
        parse_memory_status(args.status.as_deref())?.unwrap_or(memori_core::MemoryStatus::Active);
    let memory = engine
        .state()
        .vector_store
        .add_memory(memori_core::NewMemoryRecord {
            layer,
            scope,
            scope_id: args.scope_id.unwrap_or_else(|| "default".to_string()),
            memory_type: args.memory_type,
            title: args.title.unwrap_or_default(),
            content: args.content,
            source_type,
            source_ref: args.source_ref,
            confidence: args.confidence.unwrap_or(0.75),
            status,
            tags: args.tags,
            links: args.links,
            supersedes: args.supersedes,
            reason: args.reason.unwrap_or_else(|| "mcp_memory_add".to_string()),
            model: args.model,
        })
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "memory": memory }))
}

async fn memory_update(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<MemoryUpdateArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let status = parse_memory_status(args.status.as_deref())?;
    let memory = engine
        .state()
        .vector_store
        .update_memory(
            args.memory_id,
            memori_core::UpdateMemoryRecord {
                content: args.content,
                title: args.title,
                status,
                supersedes: args.supersedes,
                reason: args
                    .reason
                    .unwrap_or_else(|| "mcp_memory_update".to_string()),
                model: args.model,
            },
        )
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    match memory {
        Some(memory) => Ok(json!({ "memory": memory })),
        None => Err(JsonRpcError::invalid_params(format!(
            "memory not found: {}",
            args.memory_id
        ))),
    }
}

async fn memory_list_recent(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<MemoryListRecentArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let scope = parse_memory_scope(args.scope.as_deref())?;
    let memories = engine
        .state()
        .vector_store
        .list_recent_memories(scope, args.limit.unwrap_or(20))
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "memories": memories }))
}

async fn memory_get_source(
    state: ServerState,
    args: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<MemoryGetSourceArgs>(args)?;
    let engine = engine_from_state(&state).await?;
    let store = engine.state().vector_store.clone();
    let memory = store
        .get_memory_by_id(args.memory_id)
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    let lifecycle = store
        .list_memory_lifecycle_logs(Some(args.memory_id), 50)
        .await
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    Ok(json!({ "memory": memory, "lifecycle": lifecycle, "source_ref": memory.source_ref }))
}

fn get_app_settings_value() -> Result<JsonValue, JsonRpcError> {
    let settings = load_app_settings().map_err(JsonRpcError::internal_error)?;
    let watch_root =
        resolve_watch_root_from_settings(&settings).map_err(JsonRpcError::internal_error)?;
    let indexing = resolve_indexing_config(&settings);
    Ok(json!(AppSettingsDto::from_settings(
        settings,
        watch_root.to_string_lossy().to_string(),
        indexing,
    )))
}

fn to_tool_result(value: JsonValue, is_error: bool) -> Result<JsonValue, JsonRpcError> {
    let text = serde_json::to_string_pretty(&value)
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    serde_json::to_value(CallToolResult {
        content: vec![ToolContent::Text { text }],
        is_error: if is_error { Some(true) } else { None },
    })
    .map_err(|err| JsonRpcError::internal_error(err.to_string()))
}

fn tool(name: &str, description: &str, input_schema: JsonValue) -> Tool {
    Tool {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
    }
}

fn schema(required: &[&str]) -> JsonValue {
    use serde_json::Map;
    let mut props = Map::new();
    for key in required {
        let prop = match *key {
            "query" => json!({ "type": "string", "description": "Natural language query" }),
            "path" => json!({ "type": "string", "description": "File or directory path" }),
            "file_path" => json!({ "type": "string", "description": "Path to source file" }),
            "chunk_id" => json!({ "type": "integer", "description": "Chunk identifier" }),
            "citation_index" => json!({ "type": "integer", "description": "Citation number" }),
            "entity_id" => json!({ "type": "string", "description": "Graph entity identifier" }),
            "limit" => json!({ "type": "integer", "description": "Max results to return" }),
            "top_k" | "topK" => {
                json!({ "type": "integer", "description": "Number of top results" })
            }
            "lang" => {
                json!({ "type": "string", "description": "Language code (e.g. zh-CN, en-US)" })
            }
            "scope_paths" | "scopePaths" => {
                json!({ "type": "array", "items": { "type": "string" }, "description": "Limit search to these paths" })
            }
            "chunk_ids" => {
                json!({ "type": "array", "items": { "type": "integer" }, "description": "List of chunk identifiers" })
            }
            "model" => json!({ "type": "string", "description": "Model name to pull" }),
            "provider" => {
                json!({ "type": "string", "enum": ["ollama_local", "openai_compatible"], "description": "Model provider" })
            }
            "endpoint" => json!({ "type": "string", "description": "Provider endpoint URL" }),
            "api_key" => json!({ "type": "string", "description": "API key for remote provider" }),
            "indexing_mode" => {
                json!({ "type": "string", "enum": ["continuous", "manual", "scheduled"], "description": "Indexing strategy" })
            }
            "resource_budget" => {
                json!({ "type": "string", "enum": ["low", "balanced", "fast"], "description": "Resource usage level" })
            }
            "schedule_start" => {
                json!({ "type": "string", "description": "Schedule start time (HH:MM)" })
            }
            "schedule_end" => {
                json!({ "type": "string", "description": "Schedule end time (HH:MM)" })
            }
            "active_provider" => {
                json!({ "type": "string", "enum": ["ollama_local", "openai_compatible"], "description": "Active provider" })
            }
            "local_profile" => json!({ "type": "object", "description": "Local provider profile" }),
            "remote_profile" => {
                json!({ "type": "object", "description": "Remote provider profile" })
            }
            "candidates" => {
                json!({ "type": "array", "items": { "type": "string" }, "description": "Candidate tab names to rank" })
            }
            "memory_id" => json!({ "type": "integer", "description": "Memory identifier" }),
            "scope" => {
                json!({ "type": "string", "enum": ["user", "project", "session", "agent", "document"], "description": "Memory scope" })
            }
            "layer" => {
                json!({ "type": "string", "enum": ["stm", "mtm", "ltm", "graph", "policy"], "description": "Memory layer" })
            }
            "memory_type" => {
                json!({ "type": "string", "description": "Memory type such as note, summary, decision, task, risk, preference, fact" })
            }
            "content" => json!({ "type": "string", "description": "Memory content" }),
            "source_ref" => {
                json!({ "type": "string", "description": "Required provenance reference" })
            }
            _ => json!({ "type": "string" }),
        };
        props.insert(key.to_string(), prop);
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false
    })
}

fn parse_memory_scope(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryScope>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryScope>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory scope: {item}")))
        })
        .transpose()
}

fn parse_memory_layer(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryLayer>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryLayer>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory layer: {item}")))
        })
        .transpose()
}

fn parse_memory_source_type(
    value: Option<&str>,
) -> Result<Option<memori_core::MemorySourceType>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemorySourceType>().map_err(|_| {
                JsonRpcError::invalid_params(format!("invalid memory source_type: {item}"))
            })
        })
        .transpose()
}

fn parse_memory_status(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryStatus>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryStatus>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory status: {item}")))
        })
        .transpose()
}

fn optional_scope_refs(scope_paths: &[PathBuf]) -> Option<&[PathBuf]> {
    if scope_paths.is_empty() {
        None
    } else {
        Some(scope_paths)
    }
}

fn open_source_path(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open source failed: {err}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open source failed: {err}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|err| format!("open source failed: {err}"))?;
    }
    Ok(())
}
