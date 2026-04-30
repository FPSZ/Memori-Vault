}

pub(crate) async fn ask(state: ServerState, args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
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

pub(crate) async fn search(state: ServerState, args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
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

pub(crate) async fn get_source(
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

pub(crate) async fn open_source(args: Option<JsonValue>) -> Result<JsonValue, JsonRpcError> {
    let args = parse_params::<OpenSourceArgs>(args)?;
    open_source_path(&args.path).map_err(JsonRpcError::internal_error)?;
