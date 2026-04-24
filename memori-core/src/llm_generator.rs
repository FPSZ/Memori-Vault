use serde::{Deserialize, Serialize};

use crate::{EngineError, resolve_runtime_model_config_from_env};

const ANSWER_TEMPERATURE: f32 = 0.1;

#[derive(Debug, Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaMessageResponse {
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: Vec<OllamaMessage<'a>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: OllamaMessageResponse,
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

    let user_prompt = format!(
        "用户问题:\n{}\n\n向量检索文本上下文:\n{}\n\n图谱关系上下文:\n{}\n\n请给出最终答案，并尽量引用关键实体关系。",
        question, text_context, graph_context
    );

    let messages = vec![
        OllamaMessage {
            role: "system",
            content: system_prompt,
        },
        OllamaMessage {
            role: "user",
            content: &user_prompt,
        },
    ];

    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(&OpenAiChatCompletionRequest {
        model: &model,
        temperature: ANSWER_TEMPERATURE,
        messages,
    });
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
