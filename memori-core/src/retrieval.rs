use super::*;
use std::collections::HashSet;

pub(crate) fn merge_document_candidates(
    analysis: &QueryAnalysis,
    exact_docs: Vec<memori_storage::DocumentSignalMatch>,
    phrase_docs: Vec<memori_storage::DocumentSignalMatch>,
    strict_docs: Vec<memori_storage::FtsDocumentMatch>,
    broad_docs: Vec<memori_storage::FtsDocumentMatch>,
) -> Vec<DocumentCandidate> {
    let mut merged = HashMap::<String, DocumentCandidate>::new();
    let query_family = analysis.query_family;

    for (index, doc) in exact_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let is_exact_path = doc.matched_fields.iter().any(|field| field == "exact_path");
        let is_exact_symbol = doc
            .matched_fields
            .iter()
            .any(|field| field == "exact_symbol");
        let is_exact = is_exact_path || is_exact_symbol;
        let is_filename = doc
            .matched_fields
            .iter()
            .any(|field| matches!(field.as_str(), "exact_path" | "file_name" | "relative_path"));
        let weight = if is_exact_path {
            6.0
        } else if is_exact_symbol {
            5.0
        } else {
            3.0
        };
        let score = weight / (RRF_K + (index + 1) as f64);
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                entry.exact_signal_score =
                    entry.exact_signal_score.max(is_exact.then_some(doc.score));
                entry.exact_path_score = entry
                    .exact_path_score
                    .max(is_exact_path.then_some(doc.score));
                entry.exact_symbol_score = entry
                    .exact_symbol_score
                    .max(is_exact_symbol.then_some(doc.score));
                entry.document_filename_score = entry
                    .document_filename_score
                    .max(is_filename.then_some(doc.score));
                entry.has_exact_signal |= is_exact;
                entry.has_exact_path_signal |= is_exact_path;
                entry.has_exact_symbol_signal |= is_exact_symbol;
                entry.has_docs_phrase_signal |= false;
                entry.has_filename_signal |= is_filename;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: if is_exact_path {
                    "exact_path".to_string()
                } else if is_exact_symbol {
                    "exact_symbol".to_string()
                } else {
                    "filename".to_string()
                },
                document_rank: index + 1,
                document_raw_score: None,
                exact_signal_score: is_exact.then_some(doc.score),
                exact_path_score: is_exact_path.then_some(doc.score),
                exact_symbol_score: is_exact_symbol.then_some(doc.score),
                document_filename_score: is_filename.then_some(doc.score),
                document_final_score: score,
                has_exact_signal: is_exact,
                has_exact_path_signal: is_exact_path,
                has_exact_symbol_signal: is_exact_symbol,
                has_docs_phrase_signal: false,
                has_filename_signal: is_filename,
                has_strict_lexical: false,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in phrase_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let score = 4.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                entry.document_raw_score = Some(
                    entry
                        .document_raw_score
                        .map(|current| current.max(doc.score as f64))
                        .unwrap_or(doc.score as f64),
                );
                entry.has_docs_phrase_signal = true;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: "docs_phrase".to_string(),
                document_rank: index + 1,
                document_raw_score: Some(doc.score as f64),
                exact_signal_score: None,
                exact_path_score: None,
                exact_symbol_score: None,
                document_filename_score: None,
                document_final_score: score,
                has_exact_signal: false,
                has_exact_path_signal: false,
                has_exact_symbol_signal: false,
                has_docs_phrase_signal: true,
                has_filename_signal: false,
                has_strict_lexical: false,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in strict_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let score = 3.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                entry.document_raw_score = Some(
                    entry
                        .document_raw_score
                        .map(|current| current.max(doc.score))
                        .unwrap_or(doc.score),
                );
                entry.has_strict_lexical = true;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: "lexical_strict".to_string(),
                document_rank: index + 1,
                document_raw_score: Some(doc.score),
                exact_signal_score: None,
                exact_path_score: None,
                exact_symbol_score: None,
                document_filename_score: None,
                document_final_score: score,
                has_exact_signal: false,
                has_exact_path_signal: false,
                has_exact_symbol_signal: false,
                has_docs_phrase_signal: false,
                has_filename_signal: false,
                has_strict_lexical: true,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in broad_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let score = if matches!(query_family, QueryFamily::ImplementationLookup)
            && is_routing_noise_document(&doc.relative_path)
        {
            0.35 / (RRF_K + (index + 1) as f64)
        } else {
            1.0 / (RRF_K + (index + 1) as f64)
        };
        merged
            .entry(doc.file_path.clone())
            .and_modify(|entry| {
                if entry.document_raw_score.is_none() {
                    entry.document_raw_score = Some(doc.score);
                }
                entry.has_broad_lexical = true;
                entry.document_final_score += score;
            })
            .or_insert(DocumentCandidate {
                file_path: doc.file_path,
                relative_path: doc.relative_path,
                file_name: doc.file_name,
                is_code_document,
                document_reason: "lexical_broad".to_string(),
                document_rank: index + 1,
                document_raw_score: Some(doc.score),
                exact_signal_score: None,
                exact_path_score: None,
                exact_symbol_score: None,
                document_filename_score: None,
                document_final_score: score,
                has_exact_signal: false,
                has_exact_path_signal: false,
                has_exact_symbol_signal: false,
                has_docs_phrase_signal: false,
                has_filename_signal: false,
                has_strict_lexical: false,
                has_broad_lexical: true,
            });
    }

    let mut docs = merged.into_values().collect::<Vec<_>>();
    for doc in &mut docs {
        doc.document_reason = if doc.has_exact_path_signal {
            "exact_path".to_string()
        } else if doc.has_exact_symbol_signal {
            "exact_symbol".to_string()
        } else if doc.has_docs_phrase_signal {
            "docs_phrase".to_string()
        } else if (doc.has_exact_signal || doc.has_filename_signal)
            && (doc.has_strict_lexical || doc.has_broad_lexical)
        {
            "mixed".to_string()
        } else if doc.has_strict_lexical {
            "lexical_strict".to_string()
        } else if doc.has_filename_signal {
            "filename".to_string()
        } else {
            "lexical_broad".to_string()
        };
    }
    docs.sort_by(|a, b| {
        document_reason_priority(&b.document_reason, query_family)
            .cmp(&document_reason_priority(&a.document_reason, query_family))
            .then_with(|| {
                document_type_priority(b.is_code_document, query_family)
                    .cmp(&document_type_priority(a.is_code_document, query_family))
            })
            .then_with(|| b.document_final_score.total_cmp(&a.document_final_score))
            .then_with(|| {
                b.exact_path_score
                    .unwrap_or_default()
                    .cmp(&a.exact_path_score.unwrap_or_default())
            })
            .then_with(|| {
                b.exact_symbol_score
                    .unwrap_or_default()
                    .cmp(&a.exact_symbol_score.unwrap_or_default())
            })
            .then_with(|| {
                b.exact_signal_score
                    .unwrap_or_default()
                    .cmp(&a.exact_signal_score.unwrap_or_default())
            })
            .then_with(|| {
                b.document_filename_score
                    .unwrap_or_default()
                    .cmp(&a.document_filename_score.unwrap_or_default())
            })
            .then_with(|| {
                b.document_raw_score
                    .unwrap_or_default()
                    .total_cmp(&a.document_raw_score.unwrap_or_default())
            })
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });
    for (index, doc) in docs.iter_mut().enumerate() {
        doc.document_rank = index + 1;
    }
    docs
}

pub(crate) fn is_code_document_path(relative_path: &str) -> bool {
    relative_path
        .rsplit_once('.')
        .map(|(_, ext)| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "ts" | "tsx" | "js" | "jsx"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn document_type_priority(is_code_document: bool, query_family: QueryFamily) -> u8 {
    match query_family {
        QueryFamily::ImplementationLookup => u8::from(is_code_document),
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup => u8::from(!is_code_document),
    }
}

pub(crate) fn document_reason_priority(reason: &str, query_family: QueryFamily) -> u8 {
    match query_family {
        QueryFamily::ImplementationLookup => match reason {
            "scope" => 8,
            "exact_path" => 7,
            "exact_symbol" => 6,
            "mixed" => 5,
            "filename" => 4,
            "lexical_strict" => 3,
            "lexical_broad" => 2,
            "docs_phrase" => 1,
            _ => 0,
        },
        QueryFamily::DocsApiLookup => match reason {
            "scope" => 8,
            "docs_phrase" => 7,
            "mixed" => 6,
            "lexical_strict" => 5,
            "filename" => 4,
            "exact_path" => 3,
            "lexical_broad" => 2,
            "exact_symbol" => 1,
            _ => 0,
        },
        QueryFamily::DocsExplanatory => match reason {
            "scope" => 8,
            "docs_phrase" => 7,
            "mixed" => 6,
            "lexical_strict" => 5,
            "filename" => 4,
            "exact_path" => 3,
            "lexical_broad" => 2,
            "exact_symbol" => 1,
            _ => 0,
        },
    }
}

#[cfg(test)]
pub(crate) fn is_implementation_lookup(analysis: &QueryAnalysis) -> bool {
    matches!(analysis.query_family, QueryFamily::ImplementationLookup)
}

pub(crate) fn is_routing_noise_document(relative_path: &str) -> bool {
    matches!(
        relative_path.to_ascii_lowercase().as_str(),
        "readme.md" | "docs/plan.md" | "docs/tutorial.md"
    )
}

pub(crate) fn merge_chunk_evidence(
    analysis: &QueryAnalysis,
    candidate_docs: &[DocumentCandidate],
    strict_lexical_matches: Vec<memori_storage::FtsChunkMatch>,
    lexical_matches: Vec<memori_storage::FtsChunkMatch>,
    dense_matches: Vec<(DocumentChunk, f32)>,
) -> Vec<MergedEvidence> {
    let mut doc_rank_by_path = HashMap::new();
    for doc in candidate_docs {
        doc_rank_by_path.insert(
            doc.file_path.clone(),
            (
                doc.relative_path.clone(),
                doc.document_reason.clone(),
                doc.document_rank,
                doc.document_raw_score,
                doc.has_exact_signal,
                doc.has_docs_phrase_signal,
                doc.has_filename_signal,
                doc.has_strict_lexical,
            ),
        );
    }

    let mut merged = HashMap::<(String, usize), MergedEvidence>::new();
    for (index, item) in strict_lexical_matches.into_iter().enumerate() {
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_docs_phrase_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&item.file_path).cloned()
        else {
            continue;
        };
        let key = (item.file_path.clone(), item.chunk_index);
        let final_score = 2.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.lexical_strict_rank = Some(index + 1);
                entry.lexical_raw_score = Some(
                    entry
                        .lexical_raw_score
                        .map(|current| current.max(item.score))
                        .unwrap_or(item.score),
                );
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from(&item.file_path),
                    content: item.content.clone(),
                    chunk_index: item.chunk_index,
                    heading_path: item.heading_path.clone(),
                    block_kind: parse_block_kind(&item.block_kind),
                },
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_docs_phrase_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: Some(index + 1),
                lexical_broad_rank: None,
                lexical_raw_score: Some(item.score),
                dense_rank: None,
                dense_raw_score: None,
                final_score,
            });
    }

    for (index, item) in lexical_matches.into_iter().enumerate() {
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_docs_phrase_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&item.file_path).cloned()
        else {
            continue;
        };
        let key = (item.file_path.clone(), item.chunk_index);
        let final_score = 1.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.lexical_broad_rank = Some(index + 1);
                entry.lexical_raw_score = Some(
                    entry
                        .lexical_raw_score
                        .map(|current| current.max(item.score))
                        .unwrap_or(item.score),
                );
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk: DocumentChunk {
                    file_path: PathBuf::from(&item.file_path),
                    content: item.content.clone(),
                    chunk_index: item.chunk_index,
                    heading_path: item.heading_path.clone(),
                    block_kind: parse_block_kind(&item.block_kind),
                },
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_docs_phrase_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: None,
                lexical_broad_rank: Some(index + 1),
                lexical_raw_score: Some(item.score),
                dense_rank: None,
                dense_raw_score: None,
                final_score,
            });
    }

    for (index, (chunk, dense_score)) in dense_matches.into_iter().enumerate() {
        let file_path = chunk.file_path.to_string_lossy().to_string();
        let Some((
            relative_path,
            document_reason,
            document_rank,
            document_raw_score,
            document_has_exact_signal,
            document_has_docs_phrase_signal,
            document_has_filename_signal,
            document_has_strict_lexical,
        )) = doc_rank_by_path.get(&file_path).cloned()
        else {
            continue;
        };
        let key = (file_path, chunk.chunk_index);
        let final_score = 1.0 / (RRF_K + (index + 1) as f64);
        merged
            .entry(key)
            .and_modify(|entry| {
                entry.dense_rank = Some(index + 1);
                entry.dense_raw_score = Some(dense_score);
                entry.final_score += final_score;
            })
            .or_insert(MergedEvidence {
                chunk,
                relative_path,
                document_reason,
                document_rank,
                document_raw_score,
                document_has_exact_signal,
                document_has_docs_phrase_signal,
                document_has_filename_signal,
                document_has_strict_lexical,
                lexical_strict_rank: None,
                lexical_broad_rank: None,
                lexical_raw_score: None,
                dense_rank: Some(index + 1),
                dense_raw_score: Some(dense_score),
                final_score,
            });
    }

    for item in merged.values_mut() {
        if has_any_chunk_lexical(item) {
            continue;
        }
        let Some((is_strict, signal_score)) = direct_chunk_lexical_signal(analysis, &item.chunk)
        else {
            continue;
        };
        if is_strict {
            item.lexical_strict_rank = Some(DEFAULT_CHUNK_CANDIDATE_K + item.document_rank);
            item.lexical_raw_score = Some(signal_score);
            item.final_score += 0.75 / (RRF_K + DEFAULT_CHUNK_CANDIDATE_K as f64);
        } else {
            item.lexical_broad_rank = Some(DEFAULT_CHUNK_CANDIDATE_K + item.document_rank);
            item.lexical_raw_score = Some(signal_score);
            item.final_score += 0.35 / (RRF_K + DEFAULT_CHUNK_CANDIDATE_K as f64);
        }
    }

    let mut items = merged.into_values().collect::<Vec<_>>();
    items.sort_by(|a, b| {
        a.document_rank
            .cmp(&b.document_rank)
            .then_with(|| b.final_score.total_cmp(&a.final_score))
            .then_with(|| a.chunk.chunk_index.cmp(&b.chunk.chunk_index))
    });
    items
}

pub(crate) fn should_refuse_for_insufficient_evidence(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    if evidence.is_empty() {
        return true;
    }
    if matches!(
        analysis.query_intent,
        QueryIntent::ExternalFact | QueryIntent::SecretRequest | QueryIntent::MissingFileLookup
    ) {
        return true;
    }
    if should_force_missing_file_lookup(analysis, evidence) {
        return true;
    }

    let Some(top) = evidence.first() else {
        return true;
    };
    let top_doc_path = top.chunk.file_path.to_string_lossy().to_string();
    let top_doc_evidence = evidence
        .iter()
        .filter(|item| item.chunk.file_path.to_string_lossy() == top_doc_path)
        .collect::<Vec<_>>();
    let top_doc_count = top_doc_evidence.len();
    let top_doc_any_lexical = top_doc_evidence
        .iter()
        .filter(|item| has_any_chunk_lexical(item))
        .count();
    let top_doc_strict_lexical = top_doc_evidence
        .iter()
        .filter(|item| item.lexical_strict_rank.is_some())
        .count();
    let query_is_long =
        analysis.normalized_query.chars().count() >= 8 || analysis.flags.token_count >= 3;

    if top.lexical_strict_rank.is_some() && top_doc_count >= 2 && has_strong_document_signal(top) {
        return false;
    }

    if top.document_rank == 1
        && has_strong_document_signal(top)
        && has_any_chunk_lexical(top)
        && evidence.len() >= 2
    {
        return false;
    }

    if matches!(
        analysis.query_family,
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup
    ) && top.document_rank <= 3
        && (top.document_has_docs_phrase_signal || top.document_has_strict_lexical)
        && evidence.iter().take(2).any(has_any_chunk_lexical)
    {
        return false;
    }

    if analysis.flags.is_lookup_like
        && top.document_has_filename_signal
        && has_any_chunk_lexical(top)
        && top_doc_count >= 2
    {
        return false;
    }

    if !analysis.flags.is_lookup_like && top_doc_any_lexical >= 2 && top.document_rank <= 3 {
        return false;
    }

    if top_doc_count >= 2 && top_doc_strict_lexical >= 1 && has_strong_document_signal(top) {
        return false;
    }

    !has_any_chunk_lexical(top) && top.dense_rank.is_some() && query_is_long
}

pub(crate) fn has_any_chunk_lexical(item: &MergedEvidence) -> bool {
    item.lexical_strict_rank.is_some() || item.lexical_broad_rank.is_some()
}

pub(crate) fn has_strong_document_signal(item: &MergedEvidence) -> bool {
    item.document_has_exact_signal
        || item.document_has_docs_phrase_signal
        || item.document_has_filename_signal
        || item.document_has_strict_lexical
        || item.document_reason == "scope"
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
            if term.chars().any(is_cjk)
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

pub(crate) fn build_text_context_from_evidence(evidence: &[MergedEvidence]) -> String {
    let mut parts = Vec::with_capacity(evidence.len());
    for (index, item) in evidence.iter().enumerate() {
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
            content = item.chunk.content,
        ));
    }
    parts.join("\n\n")
}

pub(crate) fn build_reference_excerpt(file_path: &Path, chunk_content: &str) -> String {
    const TARGET_EXCERPT_CHARS: usize = 1600;

    let Ok(raw) = std::fs::read_to_string(file_path) else {
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
