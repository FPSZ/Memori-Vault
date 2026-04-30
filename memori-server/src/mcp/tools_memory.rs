}

pub(crate) async fn memory_search(
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

pub(crate) async fn memory_add(
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

pub(crate) async fn memory_update(
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

pub(crate) async fn memory_list_recent(
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

pub(crate) async fn memory_get_source(
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

pub(crate) fn parse_memory_scope(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryScope>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryScope>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory scope: {item}")))
        })
        .transpose()
}

pub(crate) fn parse_memory_layer(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryLayer>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryLayer>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory layer: {item}")))
        })
        .transpose()
}

pub(crate) fn parse_memory_source_type(
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

pub(crate) fn parse_memory_status(
    value: Option<&str>,
) -> Result<Option<memori_core::MemoryStatus>, JsonRpcError> {
    value
        .map(|item| {
            item.parse::<memori_core::MemoryStatus>()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid memory status: {item}")))
        })
