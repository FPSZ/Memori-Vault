//! 作答层 LLM-judge（审计 Q3）：把检索问答系统**实际生成的答案文本**拿去判分，
//! 补上"答得对不对"的客观闭环——此前 harness 只用 top-k 命中当代理，从不看答案。
//!
//! judge 用 chat 模型对照"期望关键事实"评定 correct/partial/incorrect 并给理由，
//! 强制 JSON 输出便于解析。judge 逻辑内聚于 core，回归 harness 与 server 评测均可复用。

use serde::{Deserialize, Serialize};

use crate::llm_http::{GenerationOptions, LlmHttpError, request_llm_text};
use crate::{EngineError, resolve_runtime_model_config_from_env};

const JUDGE_TEMPERATURE: f32 = 0.0;
const JUDGE_TIMEOUT_SECS: u64 = 60;
const JUDGE_MAX_TOKENS: u32 = 400;

/// 判定结果三档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerVerdict {
    /// 完整覆盖所有期望关键事实且无编造。
    Correct,
    /// 命中部分期望事实，或正确但不完整。
    Partial,
    /// 答错、遗漏关键事实、或编造上下文外内容。
    Incorrect,
}

impl AnswerVerdict {
    /// 用于聚合的分值：correct=1.0、partial=0.5、incorrect=0.0。
    pub fn score(self) -> f32 {
        match self {
            Self::Correct => 1.0,
            Self::Partial => 0.5,
            Self::Incorrect => 0.0,
        }
    }
}

/// judge 输出。
#[derive(Debug, Clone, Serialize)]
pub struct AnswerJudgement {
    pub verdict: AnswerVerdict,
    pub score: f32,
    pub reason: String,
}

/// judge 模型返回的原始 JSON 结构。
#[derive(Debug, Deserialize)]
struct RawJudgement {
    verdict: AnswerVerdict,
    #[serde(default)]
    reason: String,
}

/// 用 chat 模型判定 `answer` 是否正确回答了 `question`，对照 `expected_points`
/// （期望出现的关键事实 / target_clues）。返回结构化判定。
///
/// 拒答类问题请由调用方单独处理（期望"无此信息"时另走逻辑），本函数面向应答类。
pub async fn judge_answer_correctness(
    question: &str,
    expected_points: &[String],
    answer: &str,
) -> Result<AnswerJudgement, EngineError> {
    if answer.trim().is_empty() {
        return Ok(AnswerJudgement {
            verdict: AnswerVerdict::Incorrect,
            score: 0.0,
            reason: "答案为空".to_string(),
        });
    }

    let runtime = resolve_runtime_model_config_from_env();
    let model = runtime.chat_model.clone();

    let expected_block = if expected_points.is_empty() {
        "（未提供具体期望事实，请基于问题语义判断答案是否合理且无编造）".to_string()
    } else {
        expected_points
            .iter()
            .enumerate()
            .map(|(i, point)| format!("{}. {}", i + 1, point))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let system_prompt = r#"你是严格的问答评分员。给定【问题】【期望关键事实】【待评答案】，
判定待评答案是否正确覆盖了期望关键事实。评分标准：
- correct：完整覆盖全部期望关键事实，且未编造期望之外的关键结论。
- partial：仅覆盖部分期望事实，或方向正确但关键数值/约束缺失或含糊。
- incorrect：答错、遗漏全部关键事实、或编造与期望冲突的内容。
只输出 JSON，形如 {"verdict":"correct|partial|incorrect","reason":"简短中文理由"}。不要输出其它内容。"#;

    let mut user_prompt = format!(
        "【问题】\n{question}\n\n【期望关键事实】\n{expected_block}\n\n【待评答案】\n{answer}\n\n请输出 JSON 判定。"
    );
    if is_qwen_thinking_model(&model) {
        user_prompt = format!("/no_think\n\n{user_prompt}");
    }

    let raw = request_llm_text(
        &runtime,
        &runtime.chat_endpoint,
        &model,
        JUDGE_TEMPERATURE,
        system_prompt,
        &user_prompt,
        JUDGE_TIMEOUT_SECS,
        GenerationOptions {
            max_tokens: Some(JUDGE_MAX_TOKENS),
            json_object: true,
        },
    )
    .await
    .map_err(judge_error_from_llm_http)?;

    let parsed = parse_judgement(&raw);
    Ok(parsed)
}

/// 从（可能带噪声的）模型输出里抽出 JSON 并解析；解析失败时回退到关键词启发，
/// 仍失败则保守判 incorrect（不放过疑似错误答案）。
fn parse_judgement(raw: &str) -> AnswerJudgement {
    if let Some(json_slice) = extract_json_object(raw)
        && let Ok(parsed) = serde_json::from_str::<RawJudgement>(json_slice)
    {
        return AnswerJudgement {
            verdict: parsed.verdict,
            score: parsed.verdict.score(),
            reason: parsed.reason,
        };
    }
    // 回退：关键词启发。
    let lower = raw.to_ascii_lowercase();
    let verdict = if lower.contains("incorrect") {
        AnswerVerdict::Incorrect
    } else if lower.contains("partial") {
        AnswerVerdict::Partial
    } else if lower.contains("correct") {
        AnswerVerdict::Correct
    } else {
        AnswerVerdict::Incorrect
    };
    AnswerJudgement {
        verdict,
        score: verdict.score(),
        reason: format!(
            "JSON 解析失败，按关键词回退判定；原始输出片段: {}",
            truncate(raw, 160)
        ),
    }
}

/// 提取第一个平衡花括号 JSON 对象子串。
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    for (offset, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + offset + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect::<String>() + "…"
}

fn is_qwen_thinking_model(model: &str) -> bool {
    let normalized = model.to_ascii_lowercase();
    normalized.contains("qwen3") || normalized.contains("qwq")
}

fn judge_error_from_llm_http(err: LlmHttpError) -> EngineError {
    match err {
        LlmHttpError::Request(err) => EngineError::AnswerGenerateRequest(err),
        LlmHttpError::HttpStatus { status, body } => {
            EngineError::AnswerGenerateHttpStatus { status, body }
        }
        LlmHttpError::Deserialize(err) => EngineError::AnswerGenerateDeserialize(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let j = parse_judgement(r#"{"verdict":"correct","reason":"覆盖全部事实"}"#);
        assert_eq!(j.verdict, AnswerVerdict::Correct);
        assert_eq!(j.score, 1.0);
        assert_eq!(j.reason, "覆盖全部事实");
    }

    #[test]
    fn extracts_json_from_noisy_output() {
        let j = parse_judgement(
            "这是我的判断：\n{\"verdict\":\"partial\",\"reason\":\"缺数值\"}\n谢谢",
        );
        assert_eq!(j.verdict, AnswerVerdict::Partial);
        assert_eq!(j.score, 0.5);
    }

    #[test]
    fn falls_back_to_keyword_then_incorrect() {
        // 无 JSON、含 incorrect 关键词 → incorrect。
        let j = parse_judgement("the answer is incorrect because ...");
        assert_eq!(j.verdict, AnswerVerdict::Incorrect);
        // 完全无法判断 → 保守 incorrect。
        let j2 = parse_judgement("???");
        assert_eq!(j2.verdict, AnswerVerdict::Incorrect);
    }

    #[test]
    fn empty_answer_is_incorrect() {
        // 通过公开入口的快路径（无需模型）。
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let j = rt
            .block_on(judge_answer_correctness("q", &["fact".to_string()], "   "))
            .unwrap();
        assert_eq!(j.verdict, AnswerVerdict::Incorrect);
    }
}
