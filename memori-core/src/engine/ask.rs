use super::*;

pub async fn build_answer_question(
    question: &QueryAnalysis,
    lang: Option<&str>,
) -> String {
    let language = lang.unwrap_or("en-US");
    let question_text = question.canonical_question_or_original();

    match language {
        "zh-CN" => format!("基于以下上下文信息，用中文回答问题：{}", question_text),
        _ => format!("Answer the question based on the following context: {}", question_text),
    }
}

pub fn build_text_context_from_evidence(evidence: &[EvidenceItem]) -> String {
    if evidence.is_empty() {
        return String::new();
    }

    let mut context = String::new();
    context.push_str("Context information:\n\n");

    for (i, item) in evidence.iter().enumerate() {
        context.push_str(&format!("[{}] {}\n\n", i + 1, item.chunk.content));
    }

    context
}

pub fn build_merged_evidence_from_items(evidence: &[EvidenceItem]) -> Vec<EvidenceItem> {
    let mut merged: Vec<EvidenceItem> = Vec::new();
    let mut seen_chunks: HashSet<String> = HashSet::new();

    for item in evidence {
        let chunk_key = format!("{}:{}", item.chunk.file_path.display(), item.chunk.chunk_index);
        if seen_chunks.insert(chunk_key) {
            merged.push(item.clone());
        }
    }

    merged
}
