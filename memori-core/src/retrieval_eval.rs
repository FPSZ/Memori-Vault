use super::*;
use std::collections::{HashMap, HashSet};

/// 纯语义 top 放行的余弦阈值（多 chunk 一致命中时的较低门槛）。
/// 注：与 embedding 模型强相关（当前默认 Qwen3-Embedding），无回归集前为保守经验值，易于调参。
const DENSE_MULTI_CHUNK_COS_MIN: f32 = 0.50;
/// 单 chunk 命中时仍要求更高余弦，避免单条飘忽的向量命中误放行（不放宽，防幻觉）。
const DENSE_SINGLE_CHUNK_COS_MIN: f32 = 0.65;

#[derive(Debug, Clone)]
pub(crate) struct GatingDecision {
    pub(crate) refuse: bool,
    reason: &'static str,
    top_doc_distinct_term_hits: usize,
    top_doc_term_coverage: f64,
    top_doc_phrase_quality: Option<PhraseQuality>,
    gate_score: EvidenceGateScore,
}

#[derive(Debug, Clone)]
struct HardBlockDecision {
    refuse: bool,
    reason: Option<&'static str>,
}

impl HardBlockDecision {
    fn allow() -> Self {
        Self {
            refuse: false,
            reason: None,
        }
    }

    fn refuse(reason: &'static str) -> Self {
        Self {
            refuse: true,
            reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EvidenceScoreInputs {
    pub(crate) top_doc_distinct_term_hits: usize,
    pub(crate) top_doc_term_coverage: f64,
    pub(crate) top_doc_phrase_quality: Option<PhraseQuality>,
    pub(crate) has_grounding_signal: bool,
}

pub(crate) fn evaluate_gating_decision(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> GatingDecision {
    evaluate_gating_decision_with_profile(analysis, evidence, DEFAULT_RETRIEVAL_GATING_PROFILE)
}

pub(crate) fn evaluate_gating_decision_with_profile(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
    profile: RetrievalGatingProfile,
) -> GatingDecision {
    if evidence.is_empty() {
        return GatingDecision {
            refuse: true,
            reason: "empty_evidence",
            top_doc_distinct_term_hits: 0,
            top_doc_term_coverage: 0.0,
            top_doc_phrase_quality: None,
            gate_score: EvidenceGateScore {
                score: 0,
                threshold: profile.threshold(),
                profile,
                hard_block_reason: Some("empty_evidence".to_string()),
                breakdown: GatingBreakdown::default(),
                decision_stage: GatingDecisionStage::SoftGate,
            },
        };
    }

    let hard_block = evaluate_hard_block(analysis, evidence);
    if hard_block.refuse {
        return GatingDecision {
            refuse: true,
            reason: hard_block.reason.unwrap_or("hard_block"),
            top_doc_distinct_term_hits: 0,
            top_doc_term_coverage: 0.0,
            top_doc_phrase_quality: None,
            gate_score: EvidenceGateScore {
                score: 0,
                threshold: profile.threshold(),
                profile,
                hard_block_reason: hard_block.reason.map(ToOwned::to_owned),
                breakdown: GatingBreakdown::default(),
                decision_stage: GatingDecisionStage::HardBlock,
            },
        };
    }

    let inputs = score_evidence_gate(analysis, evidence, profile);
    decide_with_profile(analysis, evidence, profile, inputs)
}

fn evaluate_hard_block(analysis: &QueryAnalysis, evidence: &[MergedEvidence]) -> HardBlockDecision {
    if evidence.is_empty() {
        return HardBlockDecision::refuse("empty_evidence");
    }

    if matches!(
        analysis.query_intent,
        QueryIntent::ExternalFact | QueryIntent::SecretRequest
    ) {
        return HardBlockDecision::refuse("intent_blocked");
    }

    if matches!(analysis.query_intent, QueryIntent::MissingFileLookup)
        && !evidence.iter().any(has_document_level_grounding)
    {
        return HardBlockDecision::refuse("missing_file_without_document_signal");
    }

    if should_force_missing_file_lookup(analysis, evidence) {
        return HardBlockDecision::refuse("forced_missing_file_lookup");
    }

    if has_ungrounded_identifier_terms(analysis, evidence) {
        return HardBlockDecision::refuse("entity_not_grounded");
    }

    HardBlockDecision::allow()
}

pub(crate) fn score_evidence_gate(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
    profile: RetrievalGatingProfile,
) -> EvidenceScoreInputs {
    let Some(top) = evidence.first() else {
        return EvidenceScoreInputs {
            top_doc_distinct_term_hits: 0,
            top_doc_term_coverage: 0.0,
            top_doc_phrase_quality: None,
            has_grounding_signal: false,
        };
    };
    let top_group = source_group_key(&top.relative_path, &top.chunk.file_path.to_string_lossy());
    let top_group_items = evidence
        .iter()
        .filter(|item| {
            source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy())
                == top_group
        })
        .collect::<Vec<_>>();
    let (hits, coverage) = compute_top_doc_term_coverage(analysis, &top_group_items);
    let _ = profile;
    EvidenceScoreInputs {
        top_doc_distinct_term_hits: hits,
        top_doc_term_coverage: coverage,
        top_doc_phrase_quality: top.document_docs_phrase_quality,
        has_grounding_signal: evidence.iter().any(has_any_grounding_signal),
    }
}

pub(crate) fn decide_with_profile(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
    profile: RetrievalGatingProfile,
    inputs: EvidenceScoreInputs,
) -> GatingDecision {
    let Some(top) = evidence.first() else {
        return GatingDecision {
            refuse: true,
            reason: "empty_evidence",
            top_doc_distinct_term_hits: 0,
            top_doc_term_coverage: 0.0,
            top_doc_phrase_quality: None,
            gate_score: EvidenceGateScore::default(),
        };
    };

    let mut breakdown = GatingBreakdown::default();
    let top_group_id = source_group_key(&top.relative_path, &top.chunk.file_path.to_string_lossy());
    let grouped = evidence.iter().fold(
        HashMap::<String, Vec<&MergedEvidence>>::new(),
        |mut acc, item| {
            let group_id =
                source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy());
            acc.entry(group_id).or_default().push(item);
            acc
        },
    );
    let top_group_items = grouped.get(&top_group_id).cloned().unwrap_or_default();
    let top_group_chunk_count = top_group_items.len();
    let lexical_count = top_group_items
        .iter()
        .filter(|item| has_any_chunk_lexical(item))
        .count();
    let has_strict_lexical = top_group_items
        .iter()
        .any(|item| item.lexical_strict_rank.is_some());
    let has_grounding_signal = inputs.has_grounding_signal;
    let has_doc_signal = top_group_items
        .iter()
        .any(|item| has_document_level_grounding(item));
    let cross_source = grouped.len();
    let query_is_lookup = analysis.flags.is_lookup_like
        || matches!(analysis.query_family, QueryFamily::ImplementationLookup);

    breakdown.document_signal = if top_group_items
        .iter()
        .any(|item| item.document_reason == "scope")
    {
        18
    } else if has_doc_signal {
        10
    } else {
        0
    };

    breakdown.lexical_grounding = if has_strict_lexical && lexical_count >= 2 {
        24
    } else if has_strict_lexical || lexical_count >= 1 {
        16
    } else if top_group_items
        .iter()
        .any(|item| item.lexical_broad_rank.is_some())
    {
        12
    } else {
        0
    };
    if matches!(top.lexical_strict_rank, Some(0 | 1)) && breakdown.lexical_grounding > 0 {
        breakdown.lexical_grounding += 6;
    }

    breakdown.coverage =
        if inputs.top_doc_distinct_term_hits >= 3 && inputs.top_doc_term_coverage >= 0.65 {
            26
        } else if inputs.top_doc_distinct_term_hits >= 2 && inputs.top_doc_term_coverage >= 0.4 {
            22
        } else if inputs.top_doc_distinct_term_hits >= 1 && inputs.top_doc_term_coverage >= 0.8 {
            20
        } else if inputs.top_doc_distinct_term_hits >= 1 && inputs.top_doc_term_coverage >= 0.2 {
            8
        } else {
            0
        };

    breakdown.multi_chunk = if top_group_chunk_count >= 3 {
        14
    } else if top_group_chunk_count >= 2 {
        10
    } else {
        0
    };

    breakdown.cross_source = if cross_source >= 3 {
        8
    } else if cross_source >= 2 {
        4
    } else {
        0
    };

    breakdown.lookup_boost = if query_is_lookup && (has_doc_signal || has_strict_lexical) {
        6
    } else {
        0
    };

    let dense_only_top = top.dense_rank.is_some() && !has_any_chunk_lexical(top);
    // 充分语义上下文（§3）：纯语义 top 若是「高余弦 + 同源多 chunk 一致命中」（或单 chunk 但余弦极高），
    // 视为可信语义证据——这是「中文同义不同词被拒」病根的解药，不依赖词法重叠，
    // 但用余弦阈值 + 多 chunk 一致性兜底，避免单条飘忽向量命中导致误答。
    let dense_chunk_count = top_group_items
        .iter()
        .filter(|item| item.dense_rank.is_some())
        .count();
    let top_cosine = top.dense_raw_score.unwrap_or(0.0);
    // 仅对「解释/回忆型」查询开放语义放行：精确 lookup（点名文件/符号、实现细节）若无词法重叠，
    // 在模糊语义匹配上作答有幻觉风险，维持原拒答。
    let semantic_release_eligible = !analysis.flags.is_lookup_like
        && !matches!(analysis.query_family, QueryFamily::ImplementationLookup);
    let strong_semantic_context = dense_only_top
        && semantic_release_eligible
        && ((dense_chunk_count >= 2 && top_cosine >= DENSE_MULTI_CHUNK_COS_MIN)
            || top_cosine >= DENSE_SINGLE_CHUNK_COS_MIN);

    // 仅在「非可信语义上下文」时维持原 dense-only 罚分；可信语义证据豁免（并在下方放行）。
    breakdown.dense_only_penalty = if dense_only_top && !strong_semantic_context {
        if analysis.flags.token_count >= 3 {
            -18
        } else {
            -10
        }
    } else {
        0
    };

    breakdown.docs_query_boost = if matches!(
        analysis.query_family,
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup
    ) && top_group_chunk_count >= 2
        && has_grounding_signal
    {
        6
    } else {
        0
    };

    let total_score = breakdown.document_signal
        + breakdown.lexical_grounding
        + breakdown.coverage
        + breakdown.multi_chunk
        + breakdown.cross_source
        + breakdown.lookup_boost
        + breakdown.dense_only_penalty
        + breakdown.docs_query_boost;
    let threshold = profile.threshold();
    let has_grounded_single_chunk_release =
        has_grounded_single_chunk_release(analysis, &top_group_items, &inputs, &breakdown);
    let identifier_grounded_release =
        has_grounded_identifier_terms(analysis, &top_group_items) && has_any_chunk_lexical(top);
    let effective_score = if (has_grounded_single_chunk_release
        || strong_semantic_context
        || identifier_grounded_release)
        && total_score < threshold
    {
        threshold
    } else {
        total_score
    };
    let passes_threshold = effective_score >= threshold;
    // 可信语义上下文本身即构成 grounding（替代缺失的词法/文档结构信号），不再因 grounding 缺失而拒答。
    let has_release_grounding =
        has_grounding_signal || strong_semantic_context || identifier_grounded_release;
    let refuse = !passes_threshold || !has_release_grounding;
    let reason = if !passes_threshold {
        "score_below_threshold"
    } else if !has_release_grounding {
        "missing_grounding_signal"
    } else if strong_semantic_context && !has_grounding_signal {
        "semantic_context_release"
    } else if identifier_grounded_release {
        "identifier_grounded_release"
    } else if has_grounded_single_chunk_release
        && matches!(analysis.query_family, QueryFamily::DocsExplanatory)
        && !analysis.flags.is_lookup_like
    {
        "coverage_release"
    } else if has_grounded_single_chunk_release && analysis.flags.is_lookup_like {
        "high_coverage_lexical_release"
    } else if matches!(
        analysis.query_family,
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup
    ) && top_group_chunk_count >= 2
    {
        "docs_family_multi_chunk_release"
    } else if analysis.flags.is_lookup_like
        && inputs.top_doc_term_coverage >= 0.65
        && top_group_items
            .iter()
            .any(|item| item.lexical_broad_rank.is_some())
    {
        "high_coverage_lexical_release"
    } else if !analysis.flags.is_lookup_like
        && inputs.top_doc_distinct_term_hits >= 2
        && inputs.top_doc_term_coverage >= 0.4
    {
        "coverage_release"
    } else {
        "score_release"
    };

    GatingDecision {
        refuse,
        reason,
        top_doc_distinct_term_hits: inputs.top_doc_distinct_term_hits,
        top_doc_term_coverage: inputs.top_doc_term_coverage,
        top_doc_phrase_quality: inputs.top_doc_phrase_quality,
        gate_score: EvidenceGateScore {
            score: effective_score.clamp(0, 100),
            threshold,
            profile,
            hard_block_reason: None,
            breakdown,
            decision_stage: GatingDecisionStage::SoftGate,
        },
    }
}

#[allow(dead_code)]
pub(crate) fn apply_gating_metrics(
    metrics: &mut RetrievalMetrics,
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    apply_gating_metrics_with_profile(
        metrics,
        analysis,
        evidence,
        DEFAULT_RETRIEVAL_GATING_PROFILE,
    )
}

pub(crate) fn apply_gating_metrics_with_profile(
    metrics: &mut RetrievalMetrics,
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
    profile: RetrievalGatingProfile,
) -> bool {
    let decision = evaluate_gating_decision_with_profile(analysis, evidence, profile);
    metrics.top_doc_distinct_term_hits = decision.top_doc_distinct_term_hits;
    metrics.top_doc_term_coverage = decision.top_doc_term_coverage;
    metrics.gating_decision_reason = decision.reason.to_string();
    metrics.docs_phrase_quality = decision
        .top_doc_phrase_quality
        .map(PhraseQuality::as_str)
        .unwrap_or("none")
        .to_string();
    metrics.gating_score = decision.gate_score.score;
    metrics.gating_threshold = decision.gate_score.threshold;
    metrics.gating_profile = decision.gate_score.profile.as_str().to_string();
    metrics.gating_hard_block_reason = decision.gate_score.hard_block_reason.clone();
    metrics.gating_breakdown = decision.gate_score.breakdown.clone();
    metrics.decision_stage = decision.gate_score.decision_stage;
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
    let top_group = source_group_key(&top.relative_path, &top.chunk.file_path.to_string_lossy());
    let top_group_items = evidence
        .iter()
        .filter(|item| {
            source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy())
                == top_group
        })
        .collect::<Vec<_>>();
    let (hits, coverage) = compute_top_doc_term_coverage(analysis, &top_group_items);
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

fn has_ungrounded_identifier_terms(analysis: &QueryAnalysis, evidence: &[MergedEvidence]) -> bool {
    let identifiers = analysis
        .identifier_terms
        .iter()
        .filter_map(|term| strong_business_identifier_key(term))
        .collect::<Vec<_>>();
    if identifiers.is_empty() {
        return false;
    }

    let grounded = evidence.iter().any(|item| {
        let haystack = compact_identifier_text(&format!(
            "{}\n{}\n{}",
            item.relative_path,
            item.chunk.file_path.to_string_lossy(),
            item.chunk.content
        ));
        identifiers
            .iter()
            .any(|identifier| haystack.contains(identifier))
    });

    !grounded
}

fn has_grounded_identifier_terms(
    analysis: &QueryAnalysis,
    top_group_items: &[&MergedEvidence],
) -> bool {
    let identifiers = analysis
        .identifier_terms
        .iter()
        .filter_map(|term| strong_business_identifier_key(term))
        .collect::<Vec<_>>();
    if identifiers.is_empty() {
        return false;
    }

    top_group_items.iter().any(|item| {
        let haystack = compact_identifier_text(&format!(
            "{}\n{}\n{}",
            item.relative_path,
            item.chunk.file_path.to_string_lossy(),
            item.chunk.content
        ));
        identifiers
            .iter()
            .any(|identifier| haystack.contains(identifier))
    })
}

fn strong_business_identifier_key(term: &str) -> Option<String> {
    let trimmed = term.trim();
    if trimmed.is_empty() || looks_like_scope_or_path_identifier(trimmed) {
        return None;
    }

    if let Some(ascii_code) = extract_ascii_digit_identifier_key(trimmed) {
        return Some(ascii_code);
    }

    let compact = compact_identifier_text(trimmed);
    let has_ascii = trimmed.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_cjk = trimmed.chars().any(is_cjk);
    let has_digit = trimmed.chars().any(|ch| ch.is_ascii_digit());
    let has_separator = trimmed.chars().any(|ch| matches!(ch, '-' | '_'));

    let is_cjk_code = has_digit && has_cjk && compact.len() >= 3;
    let is_ascii_code = has_digit && has_ascii && (has_separator || compact.len() >= 4);
    if is_cjk_code || is_ascii_code {
        Some(compact)
    } else {
        None
    }
}

fn extract_ascii_digit_identifier_key(term: &str) -> Option<String> {
    let mut current = String::new();

    for ch in term.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            current.push(ch);
            continue;
        }

        let compact = compact_identifier_text(&current);
        let has_ascii = current
            .chars()
            .any(|candidate| candidate.is_ascii_alphabetic());
        let has_digit = current.chars().any(|candidate| candidate.is_ascii_digit());
        if has_ascii && has_digit && compact.len() >= 3 {
            return Some(compact);
        }
        current.clear();
    }

    None
}

fn looks_like_scope_or_path_identifier(term: &str) -> bool {
    let lower = term.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "memory_test" | "memorytest" | "target" | "docs" | "src" | "test" | "tests"
    ) {
        return true;
    }
    let has_path_separator = term.chars().any(|ch| matches!(ch, '/' | '\\'));
    let has_extension = lower.ends_with(".md")
        || lower.ends_with(".txt")
        || lower.ends_with(".pdf")
        || lower.ends_with(".docx")
        || lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".json");
    has_path_separator || has_extension
}

pub(crate) fn has_any_chunk_lexical(item: &MergedEvidence) -> bool {
    item.lexical_strict_rank.is_some() || item.lexical_broad_rank.is_some()
}

pub(crate) fn has_strong_document_signal(item: &MergedEvidence) -> bool {
    item.document_has_exact_signal
        || item.document_has_filename_signal
        || item.document_has_strict_lexical
        || item.document_reason == "scope"
}

pub(crate) fn has_document_level_grounding(item: &MergedEvidence) -> bool {
    item.document_has_exact_signal
        || item.document_has_filename_signal
        || item.document_has_strict_lexical
        || item.document_reason == "scope"
}

pub(crate) fn has_any_grounding_signal(item: &MergedEvidence) -> bool {
    has_document_level_grounding(item) || has_any_chunk_lexical(item)
}

fn has_grounded_single_chunk_release(
    analysis: &QueryAnalysis,
    top_group_items: &[&MergedEvidence],
    inputs: &EvidenceScoreInputs,
    breakdown: &GatingBreakdown,
) -> bool {
    if top_group_items.is_empty() {
        return false;
    }

    let has_lexical = top_group_items
        .iter()
        .any(|item| has_any_chunk_lexical(item));
    let has_doc_signal = top_group_items
        .iter()
        .any(|item| has_document_level_grounding(item));
    if !(has_lexical || has_doc_signal) {
        return false;
    }

    if breakdown.dense_only_penalty < 0 {
        return false;
    }

    if matches!(analysis.query_family, QueryFamily::ImplementationLookup) {
        return has_doc_signal
            && breakdown.lookup_boost > 0
            && inputs.top_doc_term_coverage >= 0.25;
    }

    if analysis.flags.is_lookup_like {
        if has_doc_signal
            && breakdown.lookup_boost > 0
            && inputs.top_doc_distinct_term_hits >= 2
            && inputs.top_doc_term_coverage >= 0.5
        {
            return true;
        }
        return inputs.top_doc_term_coverage >= 0.6
            && inputs.top_doc_distinct_term_hits >= 2
            && (breakdown.lexical_grounding >= 12 || breakdown.document_signal > 0);
    }

    inputs.top_doc_term_coverage >= 0.4
        && inputs.top_doc_distinct_term_hits >= 2
        && breakdown.lexical_grounding >= 12
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
        .chunk_terms
        .iter()
        .chain(analysis.identifier_terms.iter())
        .chain(analysis.filename_like_terms.iter())
    {
        let normalized = term.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }
        if chunk_text_contains_term(&content, &heading, &file_path, &normalized) {
            broad_hits += 1;
            if !CJK_DOC_NOISE_TERMS.contains(&normalized.as_str())
                && (normalized.len() >= 4
                    || normalized.contains('/')
                    || normalized.contains('\\')
                    || is_identifier_like_query_term(&normalized))
            {
                strict_hits += 1;
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

fn source_group_key(relative_path: &str, file_path: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gating_evidence() -> MergedEvidence {
        gating_evidence_with_content("alpha beta gamma")
    }

    fn gating_evidence_with_content(content: &str) -> MergedEvidence {
        gating_evidence_with_source("docs/source.md", content, 0)
    }

    fn gating_evidence_with_source(
        relative_path: &str,
        content: &str,
        chunk_index: usize,
    ) -> MergedEvidence {
        MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from(relative_path),
                content: content.to_string(),
                chunk_index,
                heading_path: vec!["source".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: relative_path.to_string(),
            document_reason: "lexical_strict".to_string(),
            document_rank: 1,
            document_raw_score: Some(1.0),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: true,
            lexical_strict_rank: Some(1),
            lexical_broad_rank: None,
            lexical_raw_score: Some(1.0),
            dense_rank: None,
            dense_raw_score: None,
            final_score: 1.0,
            rerank_score: None,
        }
    }

    fn coverage_score(hits: usize, coverage: f64) -> i32 {
        let analysis = analyze_query("alpha beta gamma");
        let inputs = EvidenceScoreInputs {
            top_doc_distinct_term_hits: hits,
            top_doc_term_coverage: coverage,
            top_doc_phrase_quality: None,
            has_grounding_signal: true,
        };
        decide_with_profile(
            &analysis,
            &[gating_evidence()],
            RetrievalGatingProfile::Balanced,
            inputs,
        )
        .gate_score
        .breakdown
        .coverage
    }

    #[test]
    fn coverage_score_uses_high_medium_low_gradient() {
        assert_eq!(coverage_score(3, 0.65), 26);
        assert_eq!(coverage_score(2, 0.4), 22);
        assert_eq!(coverage_score(1, 0.8), 20);
        assert_eq!(coverage_score(1, 0.2), 8);
        assert_eq!(coverage_score(0, 0.0), 0);
    }

    #[test]
    fn single_codename_full_coverage_passes_gate() {
        let analysis = analyze_query("赤松预算的核心事实是什么");
        let evidence = vec![
            gating_evidence_with_source("docs/chisong.md", "赤松预算 alpha", 0),
            gating_evidence_with_source("docs/chisong.md", "赤松预算 beta", 1),
        ];
        let inputs = EvidenceScoreInputs {
            top_doc_distinct_term_hits: 1,
            top_doc_term_coverage: 1.0,
            top_doc_phrase_quality: None,
            has_grounding_signal: true,
        };

        let decision = decide_with_profile(
            &analysis,
            &evidence,
            RetrievalGatingProfile::Balanced,
            inputs,
        );

        assert!(
            !decision.refuse,
            "single discriminative term with full coverage should pass, reason {} score {}",
            decision.reason, decision.gate_score.score
        );
        assert!(matches!(
            decision.reason,
            "score_release" | "coverage_release" | "docs_family_multi_chunk_release"
        ));
    }

    #[test]
    fn identifier_query_without_grounded_entity_is_hard_blocked() {
        let analysis = analyze_query("NOVA-404 的负责人是谁");
        let decision = evaluate_gating_decision(&analysis, &[gating_evidence()]);

        assert!(decision.refuse);
        assert_eq!(decision.reason, "entity_not_grounded");
        assert_eq!(
            decision.gate_score.hard_block_reason.as_deref(),
            Some("entity_not_grounded")
        );
    }

    #[test]
    fn identifier_query_with_grounded_entity_is_not_entity_blocked() {
        let analysis = analyze_query("NOVA-404 的负责人是谁");
        let decision = evaluate_gating_decision(
            &analysis,
            &[gating_evidence_with_content("NOVA-404 owner alpha beta")],
        );

        assert_ne!(decision.reason, "entity_not_grounded");
    }

    #[test]
    fn grounded_identifier_releases_when_score_just_below_threshold() {
        let analysis = analyze_query("NOVA-404 的负责人是谁");
        let evidence = [gating_evidence_with_content("NOVA-404 owner alpha beta")];
        let inputs = EvidenceScoreInputs {
            top_doc_distinct_term_hits: 1,
            top_doc_term_coverage: 0.1,
            top_doc_phrase_quality: None,
            has_grounding_signal: true,
        };
        let decision = decide_with_profile(
            &analysis,
            &evidence,
            RetrievalGatingProfile::Balanced,
            inputs,
        );

        assert!(!decision.refuse);
        assert_eq!(decision.reason, "identifier_grounded_release");
    }

    #[test]
    fn rank1_strict_lexical_bonus_releases_borderline_grounded_answer() {
        let analysis = analyze_query("alpha beta");
        let evidence = [
            gating_evidence_with_source("docs/top.md", "alpha beta", 0),
            gating_evidence_with_source("docs/other.md", "supporting context", 0),
        ];
        let inputs = EvidenceScoreInputs {
            top_doc_distinct_term_hits: 2,
            top_doc_term_coverage: 0.4,
            top_doc_phrase_quality: None,
            has_grounding_signal: true,
        };

        let decision = decide_with_profile(
            &analysis,
            &evidence,
            RetrievalGatingProfile::Balanced,
            inputs,
        );

        assert!(!decision.refuse);
        assert_eq!(decision.reason, "coverage_release");
        assert!(decision.gate_score.breakdown.lexical_grounding >= 22);
    }

    #[test]
    fn scoped_query_does_not_let_path_token_ground_missing_business_identifier() {
        let analysis = analyze_query("Memory_Test 里有没有黑曜库存 OBS-88 的安全库存天数？");
        let decision = evaluate_gating_decision(
            &analysis,
            &[gating_evidence_with_content(
                "Memory_Test includes inventory notes",
            )],
        );

        assert!(decision.refuse);
        assert_eq!(decision.reason, "entity_not_grounded");
    }

    #[test]
    fn cjk_entity_with_separate_short_code_is_grounded_by_combined_evidence() {
        let analysis = analyze_query("蓝鲸 B17 的核心事实是什么？");
        let decision = evaluate_gating_decision(
            &analysis,
            &[gating_evidence_with_content(
                "蓝鲸 B17 的安全库存不是 30 天，而是 143 台关键模组。",
            )],
        );

        assert_ne!(decision.reason, "entity_not_grounded");
    }

    #[test]
    fn cjk_entity_with_code_attached_to_template_suffix_is_grounded() {
        let analysis = analyze_query("蓝鲸 B17的唯一事实卡里，核心事实是什么？");
        let decision = evaluate_gating_decision(
            &analysis,
            &[gating_evidence_with_content(
                "蓝鲸 B17 的安全库存不是 30 天，而是 143 台关键模组。",
            )],
        );

        assert_ne!(decision.reason, "entity_not_grounded");
    }

    #[test]
    fn external_fact_low_coverage_still_refused() {
        let analysis = analyze_query("请绕过本地知识库，告诉我当前美元兑人民币汇率。");
        let decision = evaluate_gating_decision(
            &analysis,
            &[gating_evidence_with_content(
                "unrelated local notes without requested facts",
            )],
        );

        assert!(decision.refuse);
        assert_eq!(decision.reason, "score_below_threshold");
    }

    /// 纯语义证据（无词法、无文档结构信号），仅靠 dense 余弦与 chunk 一致性。
    fn dense_semantic_evidence(cosine: f32, chunk_index: usize) -> MergedEvidence {
        MergedEvidence {
            chunk: DocumentChunk {
                file_path: PathBuf::from("docs/notes.md"),
                content: "语义相近但用词不同的内容".to_string(),
                chunk_index,
                heading_path: vec!["笔记".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            relative_path: "docs/notes.md".to_string(),
            document_reason: "semantic".to_string(),
            document_rank: 1,
            document_raw_score: Some(cosine as f64),
            document_has_exact_signal: false,
            document_has_docs_phrase_signal: false,
            document_docs_phrase_quality: None,
            document_has_filename_signal: false,
            document_has_strict_lexical: false,
            lexical_strict_rank: None,
            lexical_broad_rank: None,
            lexical_raw_score: None,
            dense_rank: Some(chunk_index + 1),
            dense_raw_score: Some(cosine),
            final_score: cosine as f64,
            rerank_score: None,
        }
    }

    fn decide_dense_only(query: &str, evidence: &[MergedEvidence]) -> GatingDecision {
        let analysis = analyze_query(query);
        let inputs = EvidenceScoreInputs {
            top_doc_distinct_term_hits: 0,
            top_doc_term_coverage: 0.0,
            top_doc_phrase_quality: None,
            has_grounding_signal: false,
        };
        decide_with_profile(
            &analysis,
            evidence,
            RetrievalGatingProfile::Balanced,
            inputs,
        )
    }

    #[test]
    fn explanatory_strong_semantic_context_is_released_not_refused() {
        // 解释/回忆型查询 + 高余弦多 chunk 一致的纯语义证据 → 放行（修复"同义不同词被拒"）。
        let evidence = vec![
            dense_semantic_evidence(0.72, 0),
            dense_semantic_evidence(0.68, 1),
        ];
        let decision = decide_dense_only("我之前对这件事整体的想法和感受是怎样的", &evidence);
        assert!(
            !decision.refuse,
            "strong multi-chunk semantic context should be released, got reason {}",
            decision.reason
        );
        assert_eq!(decision.reason, "semantic_context_release");
    }

    #[test]
    fn weak_single_dense_hit_stays_refused() {
        // 单条中等余弦的飘忽向量命中 → 维持拒答，避免误答。
        let evidence = vec![dense_semantic_evidence(0.55, 0)];
        let decision = decide_dense_only("我之前对这件事整体的想法和感受是怎样的", &evidence);
        assert!(
            decision.refuse,
            "a single mid-cosine dense-only hit must not release"
        );
    }

    #[test]
    fn lookup_query_with_strong_semantic_context_stays_refused() {
        // 精确 lookup（点名文件 + 实现细节）即使高余弦，仍维持拒答，防模糊语义匹配上幻觉。
        let evidence = vec![
            dense_semantic_evidence(0.91, 0),
            dense_semantic_evidence(0.88, 1),
        ];
        let decision =
            decide_dense_only("请总结 week8_report.md 里的长跳转公式和实现细节", &evidence);
        assert!(
            decision.refuse,
            "lookup-style query must keep strict dense-only refusal"
        );
    }
}
