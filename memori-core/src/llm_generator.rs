use serde::{Deserialize, Serialize};

use crate::EngineError;

const DEFAULT_OLLAMA_CHAT_ENDPOINT: &str = "http://localhost:11434/api/chat";
const DEFAULT_ANSWER_MODEL: &str = "qwen2.5:7b";
const ANSWER_TEMPERATURE: f32 = 0.1;

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    stream: bool,
    options: OllamaChatOptions,
    messages: Vec<OllamaMessage<'a>>,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OllamaMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessageResponse,
}

#[derive(Debug, Deserialize)]
struct OllamaMessageResponse {
    content: String,
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

    let endpoint = std::env::var("MEMORI_OLLAMA_CHAT_ENDPOINT")
        .unwrap_or_else(|_| DEFAULT_OLLAMA_CHAT_ENDPOINT.to_string());
    let model =
        std::env::var("MEMORI_CHAT_MODEL").unwrap_or_else(|_| DEFAULT_ANSWER_MODEL.to_string());

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

    let body = OllamaChatRequest {
        model: &model,
        stream: false,
        options: OllamaChatOptions {
            temperature: ANSWER_TEMPERATURE,
        },
        messages: vec![
            OllamaMessage {
                role: "system",
                content: system_prompt,
            },
            OllamaMessage {
                role: "user",
                content: &user_prompt,
            },
        ],
    };

    let client = reqwest::Client::new();
    let response = client
        .post(endpoint)
        .json(&body)
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

    let parsed: OllamaChatResponse = response
        .json()
        .await
        .map_err(EngineError::AnswerGenerateDeserialize)?;
    let answer = parsed.message.content.trim();

    if answer.is_empty() {
        return Err(EngineError::AnswerGenerateEmpty);
    }

    Ok(answer.to_string())
}
