use super::*;

pub async fn search_chunks(
    state: &Arc<AppState>,
    query: &str,
    top_k: usize,
    scope_paths: Option<&[PathBuf]>,
) -> Result<Vec<(DocumentChunk, f32)>, EngineError> {
    if query.trim().is_empty() || top_k == 0 {
        return Ok(Vec::new());
    }

    ensure_search_ready(state).await?;
    let query_embedding = embed_query_cached(state, query).await?;
    let results = state
        .vector_store
        .search_similar_scoped(query_embedding, top_k, scope_paths.unwrap_or(&[]))
        .await?;

    Ok(results)
}

pub async fn embed_query_cached(
    state: &Arc<AppState>,
    query: &str,
) -> Result<Vec<f32>, EngineError> {
    let cache_key = query.trim().to_lowercase();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    {
        let cache = state.query_embedding_cache.read().await;
        if let Some(item) = cache.get(&cache_key) {
            if now - item.cached_at < QUERY_EMBEDDING_CACHE_TTL_SECS {
                return Ok(item.embedding.clone());
            }
        }
    }

    let embedding = state
        .embedding_client
        .embed_text(query)
        .await
        .map_err(|e| EngineError::Embedding(e.to_string()))?;

    let mut cache = state.query_embedding_cache.write().await;
    if cache.len() >= QUERY_EMBEDDING_CACHE_SIZE {
        if let Some(stale_key) = cache.iter().min_by_key(|(_, item)| item.cached_at) {
            cache.remove(stale_key.0);
        }
    }
    cache.insert(
        cache_key,
        EmbeddingCacheItem {
            embedding: embedding.clone(),
            cached_at: now,
        },
    );

    Ok(embedding)
}
