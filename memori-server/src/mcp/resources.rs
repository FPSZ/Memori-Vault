use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};

use super::protocol::*;
use super::{engine_from_state, normalize_mcp_top_k, parse_params};
use crate::*;

#[derive(Debug, Deserialize)]
struct ReadSourceUri {
    uri: String,
}

pub fn list_resources() -> ListResourcesResult {
    ListResourcesResult {
        resources: vec![
            resource(
                "memori://vault/stats",
                "Vault stats",
                "Vault document/chunk/graph statistics",
            ),
            resource(
                "memori://indexing/status",
                "Indexing status",
                "Current indexing and rebuild status",
            ),
            resource(
                "memori://settings/app",
                "App settings",
                "Watch root and indexing settings",
            ),
            resource(
                "memori://settings/models",
                "Model settings",
                "Local and remote model settings",
            ),
            resource(
                "memori://policy/enterprise",
                "Enterprise policy",
                "Egress and model allowlist policy",
            ),
            resource(
                "memori://memory/recent/project",
                "Recent project memories",
                "Recent audited project memories",
            ),
        ],
    }
}

pub fn list_resource_templates() -> ListResourceTemplatesResult {
    ListResourceTemplatesResult {
        resource_templates: vec![
            template(
                "memori://source/{path}",
                "Source document",
                "Read source document chunks by path",
            ),
            template(
                "memori://chunk/{chunk_id}",
                "Source chunk",
                "Read indexed chunk by id",
            ),
            template(
                "memori://search/{query}",
                "Search results",
                "Run retrieval for a query",
            ),
            template(
                "memori://graph/entity/{entity_id}",
                "Graph entity",
                "Read graph entity neighbors",
            ),
            template(
                "memori://memory/{memory_id}",
                "Memory record",
                "Read a memory and lifecycle log by id",
            ),
            template(
                "memori://memory/recent/{scope}",
                "Recent scoped memories",
                "Read recent memories by scope",
            ),
            template(
                "memori://memory/source/{source_ref}",
                "Memory source pointer",
                "Search memories by source reference",
            ),
            template(
                "memori://graph/entity/{entity_id}/timeline",
                "Graph entity timeline",
                "Read temporal graph timeline for an entity",
            ),
        ],
    }
}

pub async fn read_resource(
    state: ServerState,
    params: Option<JsonValue>,
) -> Result<JsonValue, JsonRpcError> {
    let params = parse_params::<ReadSourceUri>(params)?;
    let uri = params.uri;
    let value = match uri.as_str() {
        "memori://vault/stats" => {
            let engine = engine_from_state(&state).await?;
            json!(
                engine
                    .get_vault_stats()
                    .await
                    .map_err(|err| JsonRpcError::internal_error(err.to_string()))?
            )
        }
        "memori://indexing/status" => {
            let engine = engine_from_state(&state).await?;
            json!(
                engine
                    .get_indexing_status()
                    .await
                    .map_err(|err| JsonRpcError::internal_error(err.to_string()))?
            )
        }
        "memori://settings/app" => app_settings_json()?,
        "memori://settings/models" => json!(resolve_model_settings(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        )),
        "memori://policy/enterprise" => json!(resolve_enterprise_policy(
            &load_app_settings().map_err(JsonRpcError::internal_error)?
        )),
        "memori://memory/recent/project" => {
            let engine = engine_from_state(&state).await?;
            let memories = engine
                .state()
                .vector_store
                .list_recent_memories(Some(memori_core::MemoryScope::Project), 20)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!({ "memories": memories })
        }
        _ if uri.starts_with("memori://chunk/") => {
            let chunk_id = uri
                .trim_start_matches("memori://chunk/")
                .parse::<i64>()
                .map_err(|_| JsonRpcError::invalid_params("invalid chunk id"))?;
            let engine = engine_from_state(&state).await?;
            let store = engine.state().vector_store.clone();
            let chunk = store
                .get_chunk_by_id(chunk_id)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!({ "chunk": chunk })
        }
        _ if uri.starts_with("memori://source/") => {
            let path = uri.trim_start_matches("memori://source/");
            let engine = engine_from_state(&state).await?;
            let store = engine.state().vector_store.clone();
            let doc = store
                .get_document_by_file_path(Path::new(path))
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            let chunks = match doc.as_ref() {
                Some(doc) => store
                    .get_chunks_by_doc_id(doc.id)
                    .await
                    .map_err(|err| JsonRpcError::internal_error(err.to_string()))?,
                None => Vec::new(),
            };
            json!({ "document": doc, "chunks": chunks })
        }
        _ if uri.starts_with("memori://search/") => {
            let query = uri.trim_start_matches("memori://search/");
            let engine = engine_from_state(&state).await?;
            let inspection = engine
                .retrieve_structured(query, None, Some(normalize_mcp_top_k(None, 10)))
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!(inspection)
        }
        _ if uri.starts_with("memori://graph/entity/") => {
            let entity_id = uri.trim_start_matches("memori://graph/entity/");
            let entity_id = entity_id.trim_end_matches("/timeline");
            let engine = engine_from_state(&state).await?;
            let graph = engine
                .state()
                .vector_store
                .get_graph_neighbors(entity_id, 30)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!(graph)
        }
        _ if uri.starts_with("memori://memory/recent/") => {
            let scope = uri.trim_start_matches("memori://memory/recent/");
            let scope = scope.parse::<memori_core::MemoryScope>().map_err(|_| {
                JsonRpcError::invalid_params(format!("invalid memory scope: {scope}"))
            })?;
            let engine = engine_from_state(&state).await?;
            let memories = engine
                .state()
                .vector_store
                .list_recent_memories(Some(scope), 20)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!({ "memories": memories })
        }
        _ if uri.starts_with("memori://memory/source/") => {
            let source_ref = uri.trim_start_matches("memori://memory/source/");
            let engine = engine_from_state(&state).await?;
            let memories = engine
                .state()
                .vector_store
                .search_memories(memori_core::MemorySearchOptions {
                    query: source_ref.to_string(),
                    scope: None,
                    layer: None,
                    limit: 20,
                })
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!({ "source_ref": source_ref, "memories": memories })
        }
        _ if uri.starts_with("memori://memory/") => {
            let memory_id = uri
                .trim_start_matches("memori://memory/")
                .parse::<i64>()
                .map_err(|_| JsonRpcError::invalid_params("invalid memory id"))?;
            let engine = engine_from_state(&state).await?;
            let store = engine.state().vector_store.clone();
            let memory = store
                .get_memory_by_id(memory_id)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            let lifecycle = store
                .list_memory_lifecycle_logs(Some(memory_id), 50)
                .await
                .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
            json!({ "memory": memory, "lifecycle": lifecycle })
        }
        _ => {
            return Err(JsonRpcError::invalid_params(format!(
                "unknown resource: {uri}"
            )));
        }
    };

    let text = serde_json::to_string_pretty(&value)
        .map_err(|err| JsonRpcError::internal_error(err.to_string()))?;
    serde_json::to_value(ReadResourceResult {
        contents: vec![ResourceContent {
            uri,
            mime_type: "application/json".to_string(),
            text,
        }],
    })
    .map_err(|err| JsonRpcError::internal_error(err.to_string()))
}

fn app_settings_json() -> Result<JsonValue, JsonRpcError> {
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

fn resource(uri: &str, name: &str, description: &str) -> Resource {
    Resource {
        uri: uri.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        mime_type: "application/json".to_string(),
    }
}

fn template(uri_template: &str, name: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate {
        uri_template: uri_template.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        mime_type: "application/json".to_string(),
    }
}
