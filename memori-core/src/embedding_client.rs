use crate::{LocalModelClientError, resolve_runtime_model_config_from_env};
use tracing::warn;

/// 统一 Embedding 客户端（兼容 llama-server / vLLM / OpenAI 的 /v1/embeddings）。
#[derive(Debug, Clone)]
pub struct LocalEmbeddingClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl Default for LocalEmbeddingClient {
    fn default() -> Self {
        let runtime = resolve_runtime_model_config_from_env();
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: runtime.embed_endpoint,
            api_key: runtime.api_key,
            model: runtime.embed_model,
        }
    }
}

impl LocalEmbeddingClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: base_url.into(),
            api_key: None,
            model: model.into(),
        }
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub async fn embed_text(&self, prompt: &str) -> Result<Vec<f32>, LocalModelClientError> {
        let mut embeddings = self.embed_batch(&[prompt.to_string()]).await?;
        Ok(embeddings.pop().unwrap_or_default())
    }

    pub async fn embed_batch(
        &self,
        prompts: &[String],
    ) -> Result<Vec<Vec<f32>>, LocalModelClientError> {
        if prompts.is_empty() {
            return Ok(Vec::new());
        }

        let input = if prompts.len() == 1 {
            serde_json::Value::String(prompts[0].clone())
        } else {
            serde_json::Value::Array(
                prompts
                    .iter()
                    .map(|item| serde_json::Value::String(item.clone()))
                    .collect(),
            )
        };

        match self.embed_request(input, prompts.len()).await {
            Ok(embeddings) => Ok(embeddings),
            Err(err) if prompts.len() > 1 && should_retry_embeddings_individually(&err) => {
                warn!(
                    error = %err,
                    batch_size = prompts.len(),
                    "batch embedding failed; falling back to single-item requests"
                );
                let mut embeddings = Vec::with_capacity(prompts.len());
                for prompt in prompts {
                    embeddings.push(
                        self.embed_request(serde_json::Value::String(prompt.clone()), 1)
                            .await?
                            .into_iter()
                            .next()
                            .unwrap_or_default(),
                    );
                }
                Ok(embeddings)
            }
            Err(err) => Err(err),
        }
    }

    async fn embed_request(
        &self,
        input: serde_json::Value,
        expected_count: usize,
    ) -> Result<Vec<Vec<f32>>, LocalModelClientError> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(url).json(&OpenAiEmbeddingRequest {
            model: &self.model,
            input,
        });
        if let Some(key) = self.api_key.as_ref() {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .await
            .map_err(LocalModelClientError::Request)?;
        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => text,
                Err(err) => format!("<璇诲彇鍝嶅簲浣撳け璐? {err}>"),
            };

            return Err(LocalModelClientError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed: OpenAiEmbeddingResponse = response
            .json()
            .await
            .map_err(LocalModelClientError::Request)?;

        let mut data = parsed.data;
        if data.iter().all(|item| item.index.is_some()) {
            data.sort_by_key(|item| item.index.unwrap_or(usize::MAX));
        }
        let embeddings = data
            .into_iter()
            .map(|item| item.embedding)
            .collect::<Vec<_>>();
        if embeddings.len() != expected_count {
            return Err(LocalModelClientError::EmbeddingCountMismatch {
                expected: expected_count,
                actual: embeddings.len(),
            });
        }
        if embeddings.iter().any(|embedding| embedding.is_empty()) {
            return Err(LocalModelClientError::EmptyEmbedding);
        }
        Ok(embeddings)
    }
}

fn should_retry_embeddings_individually(err: &LocalModelClientError) -> bool {
    match err {
        LocalModelClientError::HttpStatus { status, .. } => {
            matches!(*status, 400 | 404 | 405 | 422 | 500)
        }
        LocalModelClientError::EmbeddingCountMismatch { .. } => true,
        LocalModelClientError::Request(_) | LocalModelClientError::EmptyEmbedding => false,
    }
}

#[derive(Debug, serde::Serialize)]
struct OpenAiEmbeddingRequest<'a> {
    model: &'a str,
    input: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingItem>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingItem {
    index: Option<usize>,
    embedding: Vec<f32>,
}
