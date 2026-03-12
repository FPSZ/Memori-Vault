use super::*;

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            records: RwLock::new(Vec::new()),
        }
    }

    /// 用于调试/测试的记录总数。
    pub async fn len(&self) -> usize {
        self.records.read().await.len()
    }

    /// 与 `len` 成对提供，满足 clippy `len_without_is_empty` 约束。
    pub async fn is_empty(&self) -> bool {
        self.records.read().await.is_empty()
    }
}

impl VectorStore for InMemoryStore {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError> {
        let chunk_count = chunks.len();
        let embedding_count = embeddings.len();

        if chunk_count != embedding_count {
            return Err(StorageError::LengthMismatch {
                chunks: chunk_count,
                embeddings: embedding_count,
            });
        }

        let mut guard = self.records.write().await;
        for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
            guard.push(StoredVectorRecord { chunk, embedding });
        }

        info!(
            inserted = chunk_count,
            total_vectors = guard.len(),
            "成功存入 {} 条向量数据",
            chunk_count
        );

        Ok(())
    }

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError> {
        if top_k == 0 || query_embedding.is_empty() {
            return Ok(Vec::new());
        }

        let guard = self.records.read().await;
        let mut scored: Vec<(DocumentChunk, f32)> = guard
            .iter()
            .map(|record| {
                let score = cosine_similarity(&query_embedding, &record.embedding);
                (record.chunk.clone(), score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(top_k);
        Ok(scored)
    }
}

/// SQLite 持久化存储实现。
impl VectorStore for SqliteStore {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError> {
        if chunks.is_empty() {
            return Err(StorageError::EmptyChunks);
        }
        let file_path = chunks[0].file_path.clone();
        if chunks.iter().any(|chunk| chunk.file_path != file_path) {
            return Err(StorageError::MixedFilePathInBatch);
        }
        self.replace_document_index(
            &file_path,
            None,
            current_unix_timestamp_secs()?,
            "",
            chunks,
            embeddings,
        )
        .await
    }

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError> {
        self.search_similar_scoped(query_embedding, top_k, &[])
            .await
    }
}
