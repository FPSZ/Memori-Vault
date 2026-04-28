use serde::{Deserialize, Serialize};

use crate::{EngineError, ModelProvider, resolve_runtime_model_config_from_env};

const ANSWER_TEMPERATURE: f32 = 0.1;

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

/// 结合向量文本上下文与图谱上下文，生成最终回答。
pub async fn generate_answer(
    question: &str,
    text_context: &str,
    graph_context: &str,
) -> Result<String, EngineError> {
    if question.trim().is_empty() {
        return Err(EngineError::AnswerGenerateEmpty);
    }

    let runtime = resolve_runtime_model_config_from_env();
    let model = runtime.chat_model.clone();
    let endpoint = format!(
        "{}/v1/chat/completions",
        runtime.chat_endpoint.trim_end_matches('/')
    );

    let system_prompt = r#"
你是 Memori-Vault 的检索问答助手。
你必须严格基于给定上下文回答，不得编造上下文中不存在的事实。
若上下文不足以回答，请明确说“当前上下文不足”并指出缺失信息。
输出要求：简洁、准确，优先中文回答。
"#;

    let mut user_prompt = format!(
        "用户问题:\n{}\n\n向量检索文本上下文:\n{}\n\n图谱关系上下文:\n{}\n\n请给出最终答案，并尽量引用关键实体关系。",
        question, text_context, graph_context
    );

    if is_qwen_thinking_model(&model) {
        // Qwen3/QwQ chat templates understand `/no_think`; this keeps local
        // answers fast even when the serving stack ignores request-level flags.
        user_prompt = format!("/no_think\n\n{user_prompt}");
    }

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

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut request_body = OpenAiChatCompletionRequest {
        model: &model,
        temperature: ANSWER_TEMPERATURE,
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
        .map_err(EngineError::AnswerGenerateRequest)?;

    let status = response.status();
    if !status.is_success() {
        let body = match response.text().await {
            Ok(text) => text,
            Err(err) => format!("<读取响应体失败: {err}>"),
        };
        return Err(EngineError::AnswerGenerateHttpStatus {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: OpenAiChatCompletionResponse = response
        .json()
        .await
        .map_err(EngineError::AnswerGenerateDeserialize)?;
    let answer = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();
    let answer = answer.trim();

    if answer.is_empty() {
        return Err(EngineError::AnswerGenerateEmpty);
    }

    Ok(answer.to_string())
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
