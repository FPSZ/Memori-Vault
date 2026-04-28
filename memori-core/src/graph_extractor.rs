use std::collections::HashSet;

use memori_storage::{GraphEdge, GraphNode};
use serde::{Deserialize, Serialize};

use crate::{EngineError, ModelProvider, resolve_runtime_model_config_from_env};

const GRAPH_TEMPERATURE: f32 = 0.0;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphData {
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<QwenChatTemplateKwargs>,
}

#[derive(Debug, Serialize)]
struct QwenChatTemplateKwargs {
    enable_thinking: bool,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: ChatMessageResponse,
}

/// 从文本块中抽取实体与关系。
///
/// 关键点：
/// - 要求本地 OpenAI-compatible 服务返回 JSON
/// - temperature 固定为 0.0，减少幻觉与结构漂移
/// - 解析失败时返回 EngineError，不允许 panic
pub async fn extract_entities(text_chunk: &str) -> Result<GraphData, EngineError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let runtime = resolve_runtime_model_config_from_env();
    let model = runtime.graph_model.clone();

    let system_prompt = r#"
你是图谱抽取器。只允许输出严格 JSON，不要输出任何额外解释。
任务：从输入文本中抽取实体节点与关系边。

输出格式必须是：
{
  "nodes": [
    {"id":"...", "label":"...", "name":"...", "description":"..."}
  ],
  "edges": [
    {"id":"...", "source_node":"...", "target_node":"...", "relation":"..."}
  ]
}

规则：
1) id 使用稳定字符串，可用小写加下划线。
2) 节点字段至少包含 id/label/name。
3) 边字段至少包含 id/source_node/target_node/relation。
4) 若无可提取内容，返回 {"nodes":[],"edges":[]}。
5) 只能输出 JSON，不得包含 markdown 代码块。
"#;

    let user_prompt = format!("请抽取以下文本中的实体与关系：\n{}", text_chunk);

    let user_prompt = if is_qwen_thinking_model(&model) {
        // Entity extraction needs strict JSON and low latency, not chain-of-thought.
        format!("/no_think\n\n{user_prompt}")
    } else {
        user_prompt
    };

    let messages = vec![
        ChatMessage {
            role: "system",
            content: system_prompt,
        },
        ChatMessage {
            role: "user",
            content: &user_prompt,
        },
    ];

    let endpoint = format!(
        "{}/v1/chat/completions",
        runtime.graph_endpoint.trim_end_matches('/')
    );
    let mut request_body = OpenAiChatCompletionRequest {
        model: &model,
        temperature: GRAPH_TEMPERATURE,
        messages,
        think: None,
        thinking: None,
        enable_thinking: None,
        chat_template_kwargs: None,
    };
    disable_qwen_thinking_if_needed(&model, &endpoint, runtime.provider, &mut request_body);
    let mut request = client.post(endpoint).json(&request_body);
    if let Some(key) = runtime.api_key.as_ref() {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(EngineError::GraphExtractRequest)?;

    let status = response.status();
    if !status.is_success() {
        let body = match response.text().await {
            Ok(text) => text,
            Err(err) => format!("<读取响应体失败: {err}>"),
        };
        return Err(EngineError::GraphExtractHttpStatus {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: OpenAiChatCompletionResponse = response
        .json()
        .await
        .map_err(EngineError::GraphExtractDeserialize)?;
    let raw_content = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();

    parse_graph_data(&raw_content)
}

fn is_qwen_thinking_model(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.contains("qwen3") || normalized.contains("qwq")
}

fn disable_qwen_thinking_if_needed(
    model: &str,
    endpoint: &str,
    provider: ModelProvider,
    request: &mut OpenAiChatCompletionRequest<'_>,
) {
    if !is_qwen_thinking_model(model) || !should_send_thinking_flags(endpoint, provider) {
        return;
    }

    // Different local OpenAI-compatible servers expose different knobs.
    // llama.cpp ignores unknown JSON fields, while Qwen-compatible templates
    // can use `chat_template_kwargs.enable_thinking`.
    request.think = Some(false);
    request.thinking = Some(false);
    request.enable_thinking = Some(false);
    request.chat_template_kwargs = Some(QwenChatTemplateKwargs {
        enable_thinking: false,
    });
}

fn should_send_thinking_flags(endpoint: &str, provider: ModelProvider) -> bool {
    provider == ModelProvider::LlamaCppLocal
        || endpoint.contains("127.0.0.1")
        || endpoint.contains("localhost")
        || endpoint.contains("0.0.0.0")
}

fn parse_graph_data(raw: &str) -> Result<GraphData, EngineError> {
    let trimmed = raw.trim();

    match serde_json::from_str::<GraphData>(trimmed) {
        Ok(data) => Ok(normalize_graph_data(data)),
        Err(primary_err) => {
            if let Some(candidate) = extract_json_object(trimmed) {
                match serde_json::from_str::<GraphData>(candidate) {
                    Ok(data) => Ok(normalize_graph_data(data)),
                    Err(fallback_err) => Err(EngineError::GraphExtractJson {
                        raw: truncate_for_log(trimmed, 1000),
                        source: merge_parse_error(primary_err, fallback_err),
                    }),
                }
            } else {
                Err(EngineError::GraphExtractJson {
                    raw: truncate_for_log(trimmed, 1000),
                    source: primary_err,
                })
            }
        }
    }
}

fn normalize_graph_data(data: GraphData) -> GraphData {
    let mut node_seen = HashSet::new();
    let mut nodes = Vec::new();

    for node in data.nodes {
        if node.id.trim().is_empty() || node.label.trim().is_empty() || node.name.trim().is_empty()
        {
            continue;
        }
        if node_seen.insert(node.id.clone()) {
            nodes.push(node);
        }
    }

    let mut edge_seen = HashSet::new();
    let mut edges = Vec::new();

    for edge in data.edges {
        if edge.id.trim().is_empty()
            || edge.source_node.trim().is_empty()
            || edge.target_node.trim().is_empty()
            || edge.relation.trim().is_empty()
        {
            continue;
        }
        if edge_seen.insert(edge.id.clone()) {
            edges.push(edge);
        }
    }

    GraphData { nodes, edges }
}

fn extract_json_object(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end < start {
        return None;
    }
    input.get(start..=end)
}

fn truncate_for_log(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let truncated: String = value.chars().take(max_chars).collect();
    format!("{}...(truncated)", truncated)
}

fn merge_parse_error(primary: serde_json::Error, fallback: serde_json::Error) -> serde_json::Error {
    if fallback.line() >= primary.line() {
        fallback
    } else {
        primary
    }
}
