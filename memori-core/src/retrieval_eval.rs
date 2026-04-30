use super::*;
use std::collections::HashSet;


#[derive(Debug, Clone)]
pub(crate) struct GatingDecision {
    pub(crate) refuse: bool,
    reason: &'static str,
    top_doc_distinct_term_hits: usize,
    top_doc_term_coverage: f64,
    top_doc_phrase_quality: Option<PhraseQuality>,
}

impl GatingDecision {
    fn allow(
        reason: &'static str,
        top_doc_distinct_term_hits: usize,
        top_doc_term_coverage: f64,
        top_doc_phrase_quality: Option<PhraseQuality>,
    ) -> Self {
        Self {
            refuse: false,
            reason,
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        }
    }

    fn refuse(
        reason: &'static str,
        top_doc_distinct_term_hits: usize,
        top_doc_term_coverage: f64,
        top_doc_phrase_quality: Option<PhraseQuality>,
    ) -> Self {
        Self {
            refuse: true,
            reason,
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        }
    }
}

pub(crate) fn evaluate_gating_decision(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> GatingDecision {
    if evidence.is_empty() {
        return GatingDecision::refuse("empty_evidence", 0, 0.0, None);
    }
    if matches!(
        analysis.query_intent,
        QueryIntent::ExternalFact | QueryIntent::SecretRequest | QueryIntent::MissingFileLookup
    ) {
        return GatingDecision::refuse("intent_blocked", 0, 0.0, None);
    }
    if should_force_missing_file_lookup(analysis, evidence) {
        return GatingDecision::refuse("forced_missing_file_lookup", 0, 0.0, None);
    }

    let Some(top) = evidence.first() else {
        return GatingDecision::refuse("empty_evidence", 0, 0.0, None);
    };
    let top_doc_path = top.chunk.file_path.to_string_lossy().to_string();
    let top_doc_evidence = evidence
        .iter()
        .filter(|item| item.chunk.file_path.to_string_lossy() == top_doc_path)
        .collect::<Vec<_>>();
    let top_doc_count = top_doc_evidence.len();
    let (top_doc_distinct_term_hits, top_doc_term_coverage) =
        compute_top_doc_term_coverage(analysis, &top_doc_evidence);
    let top_doc_strict_lexical = top_doc_evidence
        .iter()
        .filter(|item| item.lexical_strict_rank.is_some())
        .count();
    let query_is_long =
        analysis.normalized_query.chars().count() >= 8 || analysis.flags.token_count >= 3;
    let top_doc_phrase_quality = top.document_docs_phrase_quality;

    if top.lexical_strict_rank.is_some() && top_doc_count >= 2 && has_strong_document_signal(top) {
        return GatingDecision::allow(
            "strict_lexical_with_document_signal",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if top.document_rank == 1
        && has_strong_document_signal(top)
        && has_any_chunk_lexical(top)
        && evidence.len() >= 2
    {
        return GatingDecision::allow(
            "top_ranked_document_signal",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if matches!(
        analysis.query_family,
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup
    ) && top.document_rank <= 3
        && (matches!(
            top.document_docs_phrase_quality,
            Some(PhraseQuality::Specific)
        ) || top.document_has_strict_lexical)
        && evidence.iter().take(2).any(has_any_chunk_lexical)
    {
        return GatingDecision::allow(
            "docs_family_signal",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if analysis.flags.is_lookup_like
        && top.document_has_filename_signal
        && has_any_chunk_lexical(top)
        && top_doc_count >= 2
    {
        return GatingDecision::allow(
            "lookup_filename_signal",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if !analysis.flags.is_lookup_like
        && top.document_rank <= 3
        && top_doc_distinct_term_hits >= 2
        && top_doc_term_coverage >= 0.4
    {
        return GatingDecision::allow(
            "coverage_release",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if top.document_rank <= 3
        && has_any_chunk_lexical(top)
        && top_doc_distinct_term_hits >= 3
        && top_doc_term_coverage >= 0.65
    {
        return GatingDecision::allow(
            "high_coverage_lexical_release",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if top_doc_count >= 2 && top_doc_strict_lexical >= 1 && has_strong_document_signal(top) {
        return GatingDecision::allow(
            "strict_lexical_document_consensus",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    if !has_any_chunk_lexical(top) && top.dense_rank.is_some() && query_is_long {
        return GatingDecision::refuse(
            "dense_only_long_query",
            top_doc_distinct_term_hits,
            top_doc_term_coverage,
            top_doc_phrase_quality,
        );
    }

    GatingDecision::refuse(
        "insufficient_evidence",
        top_doc_distinct_term_hits,
        top_doc_term_coverage,
        top_doc_phrase_quality,
    )
}

pub(crate) fn apply_gating_metrics(
    metrics: &mut RetrievalMetrics,
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    let decision = evaluate_gating_decision(analysis, evidence);
    metrics.top_doc_distinct_term_hits = decision.top_doc_distinct_term_hits;
    metrics.top_doc_term_coverage = decision.top_doc_term_coverage;
    metrics.gating_decision_reason = decision.reason.to_string();
    metrics.docs_phrase_quality = decision
        .top_doc_phrase_quality
        .map(PhraseQuality::as_str)
        .unwrap_or("none")
        .to_string();
    decision.refuse
}

pub(crate) fn accumulate_compound_metrics(
    metrics: &mut RetrievalMetrics,
    part_metrics: &RetrievalMetrics,
) {
    metrics.doc_recall_ms += part_metrics.doc_recall_ms;
    metrics.doc_exact_ms += part_metrics.doc_exact_ms;
    metrics.doc_strict_lexical_ms += part_metrics.doc_strict_lexical_ms;
    metrics.doc_lexical_ms += part_metrics.doc_lexical_ms;
    metrics.doc_merge_ms += part_metrics.doc_merge_ms;
    metrics.chunk_strict_lexical_ms += part_metrics.chunk_strict_lexical_ms;
    metrics.chunk_lexical_ms += part_metrics.chunk_lexical_ms;
    metrics.chunk_dense_ms += part_metrics.chunk_dense_ms;
    metrics.merge_ms += part_metrics.merge_ms;
    metrics.doc_candidate_count = metrics
        .doc_candidate_count
        .max(part_metrics.doc_candidate_count);
    metrics.chunk_candidate_count = metrics
        .chunk_candidate_count
        .max(part_metrics.chunk_candidate_count);
}

pub(crate) fn dedupe_evidence_preserve_order(evidence: &mut Vec<MergedEvidence>) {
    let mut seen = HashSet::new();
    evidence.retain(|item| {
        seen.insert((
            item.chunk.file_path.to_string_lossy().to_string(),
            item.chunk.chunk_index,
        ))
    });
}

pub(crate) fn compound_part_has_grounded_evidence(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    let Some(top) = evidence.first() else {
        return false;
    };
    if !has_any_chunk_lexical(top) {
        return false;
    }
    if has_strong_document_signal(top) {
        return true;
    }
    let top_doc_path = top.chunk.file_path.to_string_lossy().to_string();
    let top_doc_evidence = evidence
        .iter()
        .filter(|item| item.chunk.file_path.to_string_lossy() == top_doc_path)
        .collect::<Vec<_>>();
    let (hits, coverage) = compute_top_doc_term_coverage(analysis, &top_doc_evidence);
    hits >= 2 && coverage >= 0.4
}

pub(crate) fn compute_top_doc_term_coverage(
    analysis: &QueryAnalysis,
    top_doc_evidence: &[&MergedEvidence],
) -> (usize, f64) {
    let support_terms = analysis.support_terms.as_slice();
    let support_count = support_terms.len().max(1);
    let top_doc_text = top_doc_evidence
        .iter()
        .map(|item| {
            (
                item.chunk.content.to_ascii_lowercase(),
                item.chunk.heading_path.join(" / ").to_ascii_lowercase(),
                item.chunk.file_path.to_string_lossy().to_ascii_lowercase(),
            )
        })
        .collect::<Vec<_>>();
    let distinct_hits = support_terms
        .iter()
        .filter(|term| {
            top_doc_text.iter().any(|(content, heading, file_path)| {
                chunk_text_contains_term(content, heading, file_path, term)
            })
        })
        .count();
    (distinct_hits, distinct_hits as f64 / support_count as f64)
}

pub(crate) fn has_any_chunk_lexical(item: &MergedEvidence) -> bool {
    item.lexical_strict_rank.is_some() || item.lexical_broad_rank.is_some()
}

pub(crate) fn has_strong_document_signal(item: &MergedEvidence) -> bool {
    item.document_has_exact_signal
        || item.document_has_filename_signal
        || item.document_has_strict_lexical
        || item.document_reason == "scope"
    // NOTE: docs_phrase 被故意排除在外。
    // meta-analysis 文档（如 docs/AI.md）容易产生虚假的 Specific docs_phrase 信号，
    // 如果算 strong signal 会污染 rankings 并错误穿透 gating。
}

pub(crate) fn direct_chunk_lexical_signal(
    analysis: &QueryAnalysis,
    chunk: &DocumentChunk,
) -> Option<(bool, f64)> {
    let content = chunk.content.to_ascii_lowercase();
    let heading = chunk.heading_path.join(" / ").to_ascii_lowercase();
    let file_path = chunk.file_path.to_string_lossy().to_ascii_lowercase();
    let mut strict_hits = 0_u32;
    let mut broad_hits = 0_u32;

    for term in analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
    {
        if chunk_text_contains_term(&content, &heading, &file_path, term) {
            strict_hits += 1;
        }
    }

    for term in &analysis.chunk_terms {
        if !is_direct_lexical_support_term(term) {
            continue;
        }
        if chunk_text_contains_term(&content, &heading, &file_path, term) {
            let is_noise = term.chars().any(is_cjk) && CJK_DOC_NOISE_TERMS.contains(&term.as_str());
            if is_noise {
                // 高频 CJK 噪声词只算 broad hit，防止无关文档被拉入 rankings
                broad_hits += 1;
            } else if term.chars().any(is_cjk)
                || term.chars().any(|ch| ch.is_ascii_digit())
                || term
                    .chars()
                    .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
            {
                strict_hits += 1;
            } else {
                broad_hits += 1;
            }
        }
    }

    if strict_hits > 0 {
        Some((true, strict_hits as f64))
    } else if broad_hits > 0 {
        Some((false, broad_hits as f64))
    } else {
        None
    }
}

pub(crate) fn is_direct_lexical_support_term(term: &str) -> bool {
    term.chars().any(is_cjk)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().count() >= 6
}

pub(crate) fn chunk_text_contains_term(
    content: &str,
    heading: &str,
    file_path: &str,
    term: &str,
) -> bool {
    let needle = term.trim().to_ascii_lowercase();
    !needle.is_empty()
        && (content.contains(&needle) || heading.contains(&needle) || file_path.contains(&needle))
}

pub(crate) fn should_force_missing_file_lookup(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    if !analysis.flags.is_lookup_like {
        return false;
    }

    let has_document_signal = evidence.iter().any(has_strong_document_signal);
    let lower = analysis.normalized_query.to_ascii_lowercase();
    let asks_for_content = is_direct_content_request_query(&analysis.normalized_query);
    let mentions_scope_exclusion = lower.contains("scope")
        && (analysis.normalized_query.contains("不包含")
            || lower.contains("not include")
            || lower.contains("outside scope"));
    let has_named_file_term = analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
        .any(|term| is_named_file_lookup_term(term));
    let has_requested_path_match = evidence_matches_requested_file(analysis, evidence);

    (mentions_scope_exclusion || (asks_for_content && has_named_file_term))
        && !has_requested_path_match
        || (!has_document_signal
            && asks_for_content
            && analysis
                .identifier_terms
                .iter()
                .any(|term| term.contains('.') || term.contains('/') || term.contains('\\')))
}

pub(crate) fn is_direct_content_request_query(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    [
        "summarize",
        "summary",
        "content",
        "contents",
        "帮我总结",
        "总结",
        "概括",
        "内容",
        "解释",
        "from my vault",
    ]
    .iter()
    .any(|marker| lower.contains(marker) || query.contains(marker))
}

pub(crate) fn should_mark_missing_file_lookup_intent(analysis: &QueryAnalysis) -> bool {
    is_direct_content_request_query(&analysis.normalized_query)
        && analysis
            .identifier_terms
            .iter()
            .chain(analysis.filename_like_terms.iter())
            .any(|term| is_named_file_lookup_term(term))
}

pub(crate) fn is_named_file_lookup_term(term: &str) -> bool {
    term.chars().any(is_cjk)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
}

pub(crate) fn evidence_matches_requested_file(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    let requested_terms = analysis
        .identifier_terms
        .iter()
        .chain(analysis.filename_like_terms.iter())
        .filter(|term| is_named_file_lookup_term(term))
        .map(|term| term.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    if requested_terms.is_empty() {
        return false;
    }

    evidence.iter().any(|item| {
        let relative = item.relative_path.to_ascii_lowercase();
        let file_name = item
            .chunk
            .file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        requested_terms.iter().any(|term| {
            relative.contains(term)
                || file_name == *term
                || file_name
                    .strip_suffix(".md")
                    .is_some_and(|stem| stem == term)
        })
    })
}

pub(crate) fn classify_query_intent(query: &str, flags: &QueryFlags) -> QueryIntent {
    if is_secret_request_query(query) {
        return QueryIntent::SecretRequest;
    }
    if is_external_fact_query(query) {
        return QueryIntent::ExternalFact;
    }
    if flags.is_lookup_like {
        QueryIntent::RepoLookup
    } else {
        QueryIntent::RepoQuestion
    }
}

pub(crate) fn is_external_fact_query(query: &str) -> bool {
    let lower = query.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    let role_fact_patterns = ["ceo of", "president of", "capital of", "founder of"];
    let time_sensitive_patterns = [
        "price today",
        "stock price",
        "bitcoin price",
        "btc price",
        "weather today",
        "news today",
        "today's news",
    ];

    role_fact_patterns
        .iter()
        .any(|pattern| lower.contains(pattern))
        || time_sensitive_patterns
            .iter()
            .any(|pattern| lower.contains(pattern))
        || query.contains("今天比特币价格")
        || query.contains("今天新闻")
}

pub(crate) fn is_secret_request_query(query: &str) -> bool {
    let lower = query.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    let sensitive_markers = [
        "api key",
        "apikey",
        "secret",
        "password",
        "credential",
        "credentials",
        "token",
        "密钥",
        "密码",
        "凭据",
    ];
    let request_markers = [
        "hidden",
        "show",
        "reveal",
        "export",
        "dump",
        "what is",
        "local settings",
        "显示",
        "导出",
        "隐藏",
        "本地设置",
    ];

    sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker) || query.contains(marker))
        && request_markers
            .iter()
            .any(|marker| lower.contains(marker) || query.contains(marker))
}

pub(crate) fn is_lookup_like_query(query: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('.')
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('_')
    {
        return true;
    }
    // CJK-only queries are never lookup-like (no spaces, token count is misleading)
    if trimmed.chars().all(|ch| is_cjk(ch) || ch.is_whitespace()) {
        return false;
    }
    if trimmed.chars().any(|ch| ch.is_ascii_digit()) && query_token_count(trimmed) <= 6 {
        return true;
    }
    query_token_count(trimmed) <= 3 && trimmed.chars().count() <= 48
}

pub(crate) fn classify_query_family(
    query: &str,
    raw_tokens: &[String],
    document_terms: &[String],
    filename_terms: &[String],
    identifier_terms: &[String],
    flags: &QueryFlags,
) -> QueryFamily {
    if looks_like_docs_api_lookup(query, raw_tokens) {
        return QueryFamily::DocsApiLookup;
    }

    if looks_like_implementation_lookup(
        query,
        document_terms,
        filename_terms,
        identifier_terms,
        flags,
    ) {
        return QueryFamily::ImplementationLookup;
    }

    QueryFamily::DocsExplanatory
}

pub(crate) fn looks_like_docs_api_lookup(query: &str, raw_tokens: &[String]) -> bool {
    let lower = query.to_ascii_lowercase();
    let has_http_path = has_http_method_and_path(raw_tokens);
    let docs_api_markers = [
        "return",
        "returns",
        "response",
        "responses",
        "metrics",
        "api",
        "返回",
        "返回什么",
        "返回哪些",
        "指标",
    ];
    let implementation_markers = [
        "which file",
        "which entry",
        "protocol",
        "struct",
        "enum",
        "type",
        "哪个文件",
        "哪个入口",
        "属于哪类查询",
        "怎么处理",
        "协议",
        "结构体",
        "枚举",
        "类型",
    ];

    has_http_path
        && docs_api_markers
            .iter()
            .any(|marker| lower.contains(marker) || query.contains(marker))
        && !implementation_markers
            .iter()
            .any(|marker| lower.contains(marker) || query.contains(marker))
}

pub(crate) fn looks_like_implementation_lookup(
    query: &str,
    document_terms: &[String],
    filename_terms: &[String],
    identifier_terms: &[String],
    flags: &QueryFlags,
) -> bool {
    let lower = query.to_ascii_lowercase();
    let implementation_markers = [
        "which file",
        "which entry",
        "implementation",
        "field",
        "fields",
        "symbol",
        "test",
        "tests",
        "query",
        "哪个文件",
        "哪个入口",
        "属于哪类查询",
        "怎么处理",
        "实现",
        "字段",
        "符号",
        "测试",
        "入口",
        "协议",
    ];

    implementation_markers
        .iter()
        .any(|marker| lower.contains(marker) || query.contains(marker))
        || !identifier_terms.is_empty()
        || (!filename_terms.is_empty() && flags.has_path_like_token && document_terms.len() <= 8)
        || (flags.has_ascii_identifier && flags.is_lookup_like)
}

pub(crate) fn has_http_method_and_path(raw_tokens: &[String]) -> bool {
    let has_method = raw_tokens.iter().any(|token| {
        matches!(
            token.to_ascii_lowercase().as_str(),
            "get" | "post" | "put" | "patch" | "delete"
        )
    });
    let has_path = raw_tokens.iter().any(|token| token.contains('/'));
    has_method && has_path
}

pub(crate) fn query_token_count(query: &str) -> usize {
    let mut count = 0;
    let mut in_token = false;
    for ch in query.chars() {
        let is_token = ch.is_alphanumeric() || is_cjk(ch);
        if is_token && !in_token {
            count += 1;
        }
        in_token = is_token;
    }
    count
}

pub(crate) fn is_cjk(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
        || ('\u{3400}'..='\u{4DBF}').contains(&ch)
        || ('\u{3040}'..='\u{30FF}').contains(&ch)
}

pub(crate) fn elapsed_ms_u64(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
