use super::*;
use std::collections::HashSet;

pub(crate) fn build_citations(evidence: &[MergedEvidence]) -> Vec<CitationItem> {
    let mut seen = HashSet::new();
    let mut citations = Vec::new();

    for item in evidence {
        let file_path = item.chunk.file_path.to_string_lossy().to_string();
        let excerpt = build_reference_excerpt(&item.chunk.file_path, &item.chunk.content);
        let dedupe_key = format!("{}\u{1f}{}", file_path.to_ascii_lowercase(), excerpt.trim());
        if !seen.insert(dedupe_key) {
            continue;
        }

        citations.push(CitationItem {
            index: citations.len() + 1,
            file_path,
            relative_path: item.relative_path.clone(),
            chunk_index: item.chunk.chunk_index,
            heading_path: item.chunk.heading_path.clone(),
            excerpt,
        });
    }

    citations
}

pub(crate) fn build_source_groups(
    citations: &[CitationItem],
    evidence: &[EvidenceItem],
) -> Vec<SourceGroup> {
    let mut groups = HashMap::<String, SourceGroup>::new();
    for citation in citations {
        let group_id = source_group_id(&citation.relative_path, &citation.file_path);
        let group = groups
            .entry(group_id.clone())
            .or_insert_with(|| SourceGroup {
                canonical_title: canonical_source_title(
                    &citation.relative_path,
                    &citation.file_path,
                ),
                group_id,
                file_paths: Vec::new(),
                relative_paths: Vec::new(),
                citation_indices: Vec::new(),
                evidence_count: 0,
            });
        push_unique(&mut group.file_paths, citation.file_path.clone());
        push_unique(&mut group.relative_paths, citation.relative_path.clone());
        group.citation_indices.push(citation.index);
    }

    for item in evidence {
        let group_id = source_group_id(&item.relative_path, &item.file_path);
        let group = groups
            .entry(group_id.clone())
            .or_insert_with(|| SourceGroup {
                canonical_title: canonical_source_title(&item.relative_path, &item.file_path),
                group_id,
                file_paths: Vec::new(),
                relative_paths: Vec::new(),
                citation_indices: Vec::new(),
                evidence_count: 0,
            });
        push_unique(&mut group.file_paths, item.file_path.clone());
        push_unique(&mut group.relative_paths, item.relative_path.clone());
        group.evidence_count += 1;
    }

    let mut values = groups.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| {
        a.citation_indices
            .first()
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(&b.citation_indices.first().copied().unwrap_or(usize::MAX))
            .then_with(|| a.canonical_title.cmp(&b.canonical_title))
    });
    values
}

pub(crate) fn build_evidence_items(evidence: &[MergedEvidence]) -> Vec<EvidenceItem> {
    evidence
        .iter()
        .enumerate()
        .map(|(index, item)| EvidenceItem {
            file_path: item.chunk.file_path.to_string_lossy().to_string(),
            relative_path: item.relative_path.clone(),
            chunk_index: item.chunk.chunk_index,
            heading_path: item.chunk.heading_path.clone(),
            block_kind: block_kind_label(item.chunk.block_kind).to_string(),
            document_reason: item.document_reason.clone(),
            reason: evidence_reason(item).to_string(),
            document_rank: item.document_rank,
            chunk_rank: index + 1,
            document_raw_score: item.document_raw_score,
            lexical_raw_score: item.lexical_raw_score,
            dense_raw_score: item.dense_raw_score,
            final_score: item.final_score,
            content: item.chunk.content.clone(),
        })
        .collect()
}

pub(crate) fn build_merged_evidence_from_items(items: &[EvidenceItem]) -> Vec<MergedEvidence> {
    items
        .iter()
        .map(|item| MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from(&item.file_path),
                content: item.content.clone(),
                chunk_index: item.chunk_index,
                heading_path: item.heading_path.clone(),
                block_kind: parse_block_kind(&item.block_kind),
            },
            relative_path: item.relative_path.clone(),
            document_reason: item.document_reason.clone(),
            document_rank: item.document_rank,
            document_raw_score: item.document_raw_score,
            document_has_exact_signal: matches!(
                item.document_reason.as_str(),
                "exact_path" | "exact_symbol"
            ),
            document_has_docs_phrase_signal: item.document_reason == "docs_phrase",
            document_docs_phrase_quality: (item.document_reason == "docs_phrase")
                .then_some(PhraseQuality::Generic),
            document_has_filename_signal: matches!(
                item.document_reason.as_str(),
                "filename" | "mixed"
            ),
            document_has_strict_lexical: matches!(
                item.document_reason.as_str(),
                "lexical_strict" | "mixed"
            ),
            lexical_strict_rank: matches!(item.reason.as_str(), "lexical_strict" | "mixed")
                .then_some(item.chunk_rank),
            lexical_broad_rank: (item.reason == "lexical_broad").then_some(item.chunk_rank),
            lexical_raw_score: item.lexical_raw_score,
            dense_rank: matches!(item.reason.as_str(), "dense" | "mixed")
                .then_some(item.chunk_rank),
            dense_raw_score: item.dense_raw_score,
            final_score: item.final_score,
        })
        .collect()
}

pub(crate) fn prepare_query_for_retrieval(question: &str) -> QueryPreparation {
    let query_analysis_started_at = Instant::now();
    let analysis = analyze_query(question);
    let mut metrics = RetrievalMetrics {
        query_analysis_ms: elapsed_ms_u64(query_analysis_started_at),
        query_flags: query_flags_as_labels(&analysis),
        gating_decision_reason: "not_evaluated".to_string(),
        docs_phrase_quality: "none".to_string(),
        ..RetrievalMetrics::default()
    };
    metrics
        .query_flags
        .push(format!("intent:{}", analysis.query_intent.as_str()));
    if !analysis.identifier_terms.is_empty() {
        metrics.query_flags.push(format!(
            "identifier_terms:{}",
            analysis.identifier_terms.len()
        ));
    }
    if !analysis.filename_like_terms.is_empty() {
        metrics.query_flags.push(format!(
            "filename_terms:{}",
            analysis.filename_like_terms.len()
        ));
    }
    QueryPreparation { analysis, metrics }
}

pub fn build_query_terms_for_offline_embedding(query: &str) -> Vec<String> {
    let analysis = analyze_query(query);
    let mut terms = Vec::new();
    terms.extend(analysis.chunk_terms);
    terms.extend(analysis.identifier_terms);
    terms.extend(analysis.filename_like_terms);
    if !analysis.normalized_query.is_empty() {
        terms.push(analysis.normalized_query);
    }
    if !analysis.lexical_query.is_empty() {
        terms.push(analysis.lexical_query);
    }

    let mut seen = std::collections::HashSet::new();
    terms.retain(|term| {
        let normalized = term.trim().to_ascii_lowercase();
        !normalized.is_empty() && seen.insert(normalized)
    });
    terms
}

pub(crate) fn build_text_context_from_evidence_with_budget(
    evidence: &[MergedEvidence],
    max_chars: usize,
) -> (String, usize) {
    let mut parts = Vec::with_capacity(evidence.len());
    let mut used_chars = 0usize;
    for (index, item) in evidence.iter().enumerate() {
        if used_chars >= max_chars {
            break;
        }
        let remaining = max_chars.saturating_sub(used_chars);
        let mut content = item.chunk.content.clone();
        if content.chars().count() > remaining.saturating_sub(512) {
            content = take_chars(&content, remaining.saturating_sub(512).max(400));
            content.push_str("\n...[truncated by context budget]");
        }
        let heading = if item.chunk.heading_path.is_empty() {
            String::new()
        } else {
            format!("标题路径: {}\n", item.chunk.heading_path.join(" > "))
        };
        parts.push(format!(
            "片段#{display_index}\n来源: {path}\n相对路径: {relative_path}\n块序号: {chunk_index}\n块类型: {block_kind}\n文档排序: #{document_rank}\n文档命中原因: {document_reason}\n片段排序分数: {score:.6}\n命中原因: {reason}\n{heading}内容:\n{content}",
            display_index = index + 1,
            path = item.chunk.file_path.display(),
            relative_path = item.relative_path,
            chunk_index = item.chunk.chunk_index,
            block_kind = block_kind_label(item.chunk.block_kind),
            document_rank = item.document_rank,
            document_reason = &item.document_reason,
            score = item.final_score,
            reason = evidence_reason(item),
            heading = heading,
            content = content,
        ));
        used_chars = parts.iter().map(|part| part.chars().count()).sum();
    }
    let context = parts.join("\n\n");
    let used = estimate_tokens(&context);
    (context, used)
}

pub(crate) fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}
fn take_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn push_unique(items: &mut Vec<String>, item: String) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}

fn source_group_id(relative_path: &str, file_path: &str) -> String {
    let source = if relative_path.trim().is_empty() {
        file_path
    } else {
        relative_path
    };
    let normalized = source.replace('\\', "/").to_ascii_lowercase();
    let parent = normalized
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    let file_name = normalized.rsplit('/').next().unwrap_or(&normalized);
    let stem = file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name);
    let canonical_stem = stem
        .trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '_' || ch == '-')
        .to_string();
    if parent.is_empty() {
        canonical_stem
    } else {
        format!("{parent}/{canonical_stem}")
    }
}

fn canonical_source_title(relative_path: &str, file_path: &str) -> String {
    let source = if relative_path.trim().is_empty() {
        file_path
    } else {
        relative_path
    };
    let file_name = source
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(source)
        .to_string();
    file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or(file_name)
}
pub(crate) fn build_reference_excerpt(file_path: &Path, chunk_content: &str) -> String {
    const TARGET_EXCERPT_CHARS: usize = 1600;

    let raw = if let Some(text) = memori_parser::extract_document_text(file_path) {
        text
    } else if let Ok(text) = std::fs::read_to_string(file_path) {
        text
    } else {
        return chunk_content.to_string();
    };

    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        return chunk_content.to_string();
    }

    let chunk_normalized = chunk_content.trim();
    let anchor = chunk_normalized
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.chars().count() >= 8)
        .unwrap_or(chunk_normalized);
    let paragraph_index = paragraphs
        .iter()
        .position(|paragraph| paragraph.contains(chunk_normalized))
        .or_else(|| {
            paragraphs
                .iter()
                .position(|paragraph| paragraph.contains(anchor))
        });

    let Some(index) = paragraph_index else {
        return chunk_content.to_string();
    };

    let mut start = index;
    let mut end = index + 1;
    let mut total_chars = paragraphs[index].chars().count();
    while total_chars < TARGET_EXCERPT_CHARS && (start > 0 || end < paragraphs.len()) {
        let prev_len = if start > 0 {
            paragraphs[start - 1].chars().count()
        } else {
            0
        };
        let next_len = if end < paragraphs.len() {
            paragraphs[end].chars().count()
        } else {
            0
        };
        if next_len >= prev_len && end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
            continue;
        }
        if start > 0 {
            start -= 1;
            total_chars += prev_len;
            continue;
        }
        if end < paragraphs.len() {
            total_chars += next_len;
            end += 1;
        }
    }

    paragraphs[start..end].join("\n\n")
}

pub(crate) fn build_answer_question(query: &str, lang: Option<&str>) -> String {
    match normalize_language(lang) {
        Some("zh-CN") => format!("{query}\n\n请仅使用中文回答。"),
        Some("en-US") => format!("{query}\n\nPlease answer in English only."),
        _ => query.to_string(),
    }
}

pub(crate) fn normalize_language(lang: Option<&str>) -> Option<&'static str> {
    let lang = lang?;
    let lower = lang.trim().to_ascii_lowercase();
    if lower.starts_with("zh") {
        Some("zh-CN")
    } else if lower.starts_with("en") {
        Some("en-US")
    } else {
        None
    }
}

pub(crate) fn parse_block_kind(value: &str) -> memori_parser::ChunkBlockKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "heading" => memori_parser::ChunkBlockKind::Heading,
        "list" => memori_parser::ChunkBlockKind::List,
        "code_block" => memori_parser::ChunkBlockKind::CodeBlock,
        "table" => memori_parser::ChunkBlockKind::Table,
        "quote" => memori_parser::ChunkBlockKind::Quote,
        "html" => memori_parser::ChunkBlockKind::Html,
        "thematic_break" => memori_parser::ChunkBlockKind::ThematicBreak,
        "mixed" => memori_parser::ChunkBlockKind::Mixed,
        _ => memori_parser::ChunkBlockKind::Paragraph,
    }
}

pub(crate) fn block_kind_label(kind: memori_parser::ChunkBlockKind) -> &'static str {
    match kind {
        memori_parser::ChunkBlockKind::Heading => "heading",
        memori_parser::ChunkBlockKind::Paragraph => "paragraph",
        memori_parser::ChunkBlockKind::List => "list",
        memori_parser::ChunkBlockKind::CodeBlock => "code_block",
        memori_parser::ChunkBlockKind::Table => "table",
        memori_parser::ChunkBlockKind::Quote => "quote",
        memori_parser::ChunkBlockKind::Html => "html",
        memori_parser::ChunkBlockKind::ThematicBreak => "thematic_break",
        memori_parser::ChunkBlockKind::Mixed => "mixed",
    }
}
pub(crate) fn evidence_reason(item: &MergedEvidence) -> &'static str {
    let has_strict = item.lexical_strict_rank.is_some();
    let has_broad = item.lexical_broad_rank.is_some();
    match (has_strict || has_broad, item.dense_rank.is_some()) {
        (true, true) => "mixed",
        (true, false) if has_strict => "lexical_strict",
        (true, false) => "lexical_broad",
        (false, true) => "dense",
        (false, false) => "unknown",
    }
}
