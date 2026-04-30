use super::*;

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
                docs_phrase_quality: None,
                has_filename_signal: is_filename,
                has_strict_lexical: false,
                has_broad_lexical: false,
            });
    }

    for (index, doc) in phrase_docs.into_iter().enumerate() {
        let is_code_document = is_code_document_path(&doc.relative_path);
        let phrase_quality = docs_phrase_quality_from_match(&doc);
        // docs_phrase 权重根据 query family 动态调整：
        // ImplementationLookup 场景下降低权重，防止 meta-analysis 文档压过代码文档
        let score = if matches!(query_family, QueryFamily::ImplementationLookup) {
            2.0
        } else {
            3.5
        } / (RRF_K + (index + 1) as f64);
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
                entry.docs_phrase_quality =
                    max_phrase_quality(entry.docs_phrase_quality, Some(phrase_quality));
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
                docs_phrase_quality: Some(phrase_quality),
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
                docs_phrase_quality: None,
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
                docs_phrase_quality: None,
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
        document_reason_priority(&b.document_reason, b.docs_phrase_quality, query_family)
            .cmp(&document_reason_priority(
                &a.document_reason,
                a.docs_phrase_quality,
                query_family,
            ))
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

pub(crate) fn docs_phrase_quality_from_match(
    doc: &memori_storage::DocumentSignalMatch,
) -> PhraseQuality {
    if doc.phrase_specific {
        PhraseQuality::Specific
    } else {
        PhraseQuality::Generic
    }
}

pub(crate) fn max_phrase_quality(
    left: Option<PhraseQuality>,
    right: Option<PhraseQuality>,
) -> Option<PhraseQuality> {
    match (left, right) {
        (Some(PhraseQuality::Specific), _) | (_, Some(PhraseQuality::Specific)) => {
            Some(PhraseQuality::Specific)
        }
        (Some(PhraseQuality::Generic), _) | (_, Some(PhraseQuality::Generic)) => {
            Some(PhraseQuality::Generic)
        }
        (None, None) => None,
    }
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

pub(crate) fn document_reason_priority(
    reason: &str,
    phrase_quality: Option<PhraseQuality>,
    query_family: QueryFamily,
) -> u8 {
    match query_family {
        QueryFamily::ImplementationLookup => match reason {
            "scope" => 8,
            "exact_path" => 7,
            "exact_symbol" => 6,
            "mixed" => 5,
            "filename" => 4,
            "lexical_strict" => 3,
            "lexical_broad" => 2,
            "docs_phrase" => match phrase_quality {
                Some(PhraseQuality::Specific) => 1,
                _ => 0,
            },
            _ => 0,
        },
        QueryFamily::DocsApiLookup => match reason {
            "scope" => 8,
            "docs_phrase" => match phrase_quality {
                Some(PhraseQuality::Specific) => 7,
                _ => 1,
            },
            "mixed" => 6,
            "lexical_strict" => 5,
            "filename" => 4,
            "exact_path" => 3,
            "lexical_broad" => 2,
            "exact_symbol" => 0,
            _ => 0,
        },
        QueryFamily::DocsExplanatory => match reason {
            "scope" => 8,
            "docs_phrase" => match phrase_quality {
                Some(PhraseQuality::Specific) => 7,
                _ => 1,
            },
            "mixed" => 6,
            "lexical_strict" => 5,
            "filename" => 4,
            "exact_path" => 3,
            "lexical_broad" => 2,
            "exact_symbol" => 0,
            _ => 0,
        },
    }
}

#[cfg(test)]
pub(crate) fn is_implementation_lookup(analysis: &QueryAnalysis) -> bool {
    matches!(analysis.query_family, QueryFamily::ImplementationLookup)
}

pub(crate) fn is_routing_noise_document(relative_path: &str) -> bool {
    let lower = relative_path.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "readme.md"
            | "docs/plan.md"
            | "docs/planning/plan.md"
            | "docs/tutorial.md"
            | "docs/guides/tutorial.md"
    ) || lower.starts_with("docs/ai")  // docs/AI.md, docs/ai-overview.md 等 meta-analysis
        || lower == "docs/structure.md"
        || lower == "docs/architecture/structure.md"
        || lower == "docs/architecture.md"
        || lower == "docs/overview.md"
        || lower == "docs/design.md"
        || lower == "docs/roadmap.md"
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
                doc.docs_phrase_quality,
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
            document_docs_phrase_quality,
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
                document_docs_phrase_quality,
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
            document_docs_phrase_quality,
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
                document_docs_phrase_quality,
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
            document_docs_phrase_quality,
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
                document_docs_phrase_quality,
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

#[cfg(test)]
pub(crate) fn should_refuse_for_insufficient_evidence(
    analysis: &QueryAnalysis,
    evidence: &[MergedEvidence],
) -> bool {
    evaluate_gating_decision(analysis, evidence).refuse
}
