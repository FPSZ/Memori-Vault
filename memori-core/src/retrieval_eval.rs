use super::*;
use std::collections::{HashMap, HashSet};

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
    let decision = decide_with_profile(analysis, evidence, profile, inputs);
    decision
}

fn evaluate_hard_block(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> HardBlockDecision {
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
        .filter(|item| source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy()) == top_group)
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
    let grouped = evidence
        .iter()
        .fold(HashMap::<String, Vec<&MergedEvidence>>::new(), |mut acc, item| {
            let group_id = source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy());
            acc.entry(group_id).or_default().push(item);
            acc
        });
    let top_group_items = grouped.get(&top_group_id).cloned().unwrap_or_default();
    let top_group_chunk_count = top_group_items.len();
    let lexical_count = top_group_items.iter().filter(|item| has_any_chunk_lexical(item)).count();
    let has_strict_lexical = top_group_items
        .iter()
        .any(|item| item.lexical_strict_rank.is_some());
    let has_grounding_signal = inputs.has_grounding_signal;
    let has_doc_signal = top_group_items.iter().any(|item| has_document_level_grounding(item));
    let cross_source = grouped.len();
    let query_is_lookup = analysis.flags.is_lookup_like
        || matches!(analysis.query_family, QueryFamily::ImplementationLookup);

    breakdown.document_signal = if top_group_items.iter().any(|item| item.document_reason == "scope") {
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
    } else if top_group_items.iter().any(|item| item.lexical_broad_rank.is_some()) {
        12
    } else {
        0
    };

    breakdown.coverage = if inputs.top_doc_distinct_term_hits >= 3 && inputs.top_doc_term_coverage >= 0.65 {
        22
    } else if inputs.top_doc_distinct_term_hits >= 2 && inputs.top_doc_term_coverage >= 0.4 {
        22
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
    breakdown.dense_only_penalty = if dense_only_top && analysis.flags.token_count >= 3 {
        -18
    } else if dense_only_top {
        -10
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
    let has_grounded_single_chunk_release = has_grounded_single_chunk_release(
        analysis,
        &top_group_items,
        &inputs,
        &breakdown,
    );
    let effective_score = if has_grounded_single_chunk_release && total_score < threshold {
        threshold
    } else {
        total_score
    };
    let passes_threshold = effective_score >= threshold;
    let refuse = !passes_threshold || !has_grounding_signal;
    let reason = if !passes_threshold {
        "score_below_threshold"
    } else if !has_grounding_signal {
        "missing_grounding_signal"
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
        && top_group_items.iter().any(|item| item.lexical_broad_rank.is_some())
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
        .filter(|item| source_group_key(&item.relative_path, &item.chunk.file_path.to_string_lossy()) == top_group)
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

    let has_lexical = top_group_items.iter().any(|item| has_any_chunk_lexical(item));
    let has_doc_signal = top_group_items.iter().any(|item| has_document_level_grounding(item));
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
            if normalized.len() >= 4 || normalized.contains('/') || normalized.contains('\\') {
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
