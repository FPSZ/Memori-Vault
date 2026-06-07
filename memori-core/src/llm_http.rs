use serde::{Deserialize, Serialize};

use crate::{ChatApiFormat, ModelProvider, RuntimeModelConfig, build_openai_url};

#[derive(Debug)]
pub(crate) enum LlmHttpError {
    Request(reqwest::Error),
    HttpStatus { status: u16, body: String },
    Deserialize(reqwest::Error),
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
struct JsonResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<JsonResponseFormat>,
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
struct OpenAiResponsesRequest<'a> {
    model: &'a str,
    temperature: f32,
    instructions: &'a str,
    input: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
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

#[derive(Debug, Deserialize)]
struct OpenAiResponsesResponse {
    #[serde(default)]
    output_text: Option<String>,
    #[serde(default)]
    output: Vec<OpenAiResponseOutputItem>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseOutputItem {
    #[serde(default)]
    content: Vec<OpenAiResponseContentItem>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseContentItem {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// Optional decoding controls shared across LLM calls.
///
/// `max_tokens` bounds the generated output (prevents runaway/looping decode on
/// dense inputs); `json_object` asks an OpenAI-compatible server to constrain the
/// output to a single JSON object (eliminates prose/markdown around the payload,
/// which otherwise wastes decode time and triggers parse-failure retries).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct GenerationOptions {
    pub max_tokens: Option<u32>,
    pub json_object: bool,
}

#[allow(clippy::too_many_arguments)] // internal HTTP helper; args are all distinct request fields
pub(crate) async fn request_llm_text(
    runtime: &RuntimeModelConfig,
    endpoint: &str,
    model: &str,
    temperature: f32,
    system: &str,
    user: &str,
    timeout_secs: u64,
    options: GenerationOptions,
) -> Result<String, LlmHttpError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut request = match runtime.api_format {
        ChatApiFormat::Responses => {
            let url = build_openai_url(endpoint, "responses");
            client.post(url).json(&OpenAiResponsesRequest {
                model,
                temperature,
                instructions: system,
                input: user,
                max_output_tokens: options.max_tokens,
            })
        }
        ChatApiFormat::Chat => {
            let url = build_openai_url(endpoint, "chat/completions");
            let mut request_body = OpenAiChatCompletionRequest {
                model,
                temperature,
                messages: vec![
                    ChatMessage {
                        role: "system",
                        content: system,
                    },
                    ChatMessage {
                        role: "user",
                        content: user,
                    },
                ],
                max_tokens: options.max_tokens,
                response_format: options.json_object.then_some(JsonResponseFormat {
                    kind: "json_object",
                }),
                think: None,
                thinking: None,
                enable_thinking: None,
                chat_template_kwargs: None,
            };
            disable_qwen_thinking_if_needed(model, endpoint, runtime.provider, &mut request_body);
            client.post(url).json(&request_body)
        }
    };

    if let Some(key) = runtime.api_key.as_ref() {
        request = request.bearer_auth(key);
    }

    let response = request.send().await.map_err(LlmHttpError::Request)?;

    let status = response.status();
    if !status.is_success() {
        let body = match response.text().await {
            Ok(text) => text,
            Err(err) => format!("<读取响应体失败: {err}>"),
        };
        return Err(LlmHttpError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }

    match runtime.api_format {
        ChatApiFormat::Responses => {
            let parsed: OpenAiResponsesResponse =
                response.json().await.map_err(LlmHttpError::Deserialize)?;
            Ok(response_output_text(parsed))
        }
        ChatApiFormat::Chat => {
            let parsed: OpenAiChatCompletionResponse =
                response.json().await.map_err(LlmHttpError::Deserialize)?;
            Ok(parsed
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.content)
                .unwrap_or_default())
        }
    }
}

fn response_output_text(parsed: OpenAiResponsesResponse) -> String {
    if let Some(text) = parsed.output_text
        && !text.trim().is_empty()
    {
        return text;
    }
    parsed
        .output
        .into_iter()
        .flat_map(|item| item.content)
        .filter(|item| {
            item.kind
                .as_deref()
                .map(|kind| kind == "output_text")
                .unwrap_or(true)
        })
        .filter_map(|item| item.text)
        .collect::<Vec<_>>()
        .join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_output_text_prefers_output_text() {
        let parsed = OpenAiResponsesResponse {
            output_text: Some("direct".to_string()),
            output: vec![OpenAiResponseOutputItem {
                content: vec![OpenAiResponseContentItem {
                    kind: Some("output_text".to_string()),
                    text: Some("nested".to_string()),
                }],
            }],
        };

        assert_eq!(response_output_text(parsed), "direct");
    }

    #[test]
    fn responses_output_text_reads_nested_output_text_items() {
        let parsed = OpenAiResponsesResponse {
            output_text: None,
            output: vec![
                OpenAiResponseOutputItem {
                    content: vec![OpenAiResponseContentItem {
                        kind: Some("reasoning".to_string()),
                        text: Some("hidden".to_string()),
                    }],
                },
                OpenAiResponseOutputItem {
                    content: vec![
                        OpenAiResponseContentItem {
                            kind: Some("output_text".to_string()),
                            text: Some("hello".to_string()),
                        },
                        OpenAiResponseContentItem {
                            kind: Some("output_text".to_string()),
                            text: Some("world".to_string()),
                        },
                    ],
                },
            ],
        };

        assert_eq!(response_output_text(parsed), "hello\nworld");
    }
}
