use super::*;

pub(crate) fn score_document_signal_match(
    signal_terms: &[String],
    file_name: &str,
    relative_path: &str,
    heading_catalog_text: &str,
    document_search_text: &str,
) -> Option<(i64, Vec<String>)> {
    let file_name_lower = file_name.to_ascii_lowercase();
    let relative_path_lower = relative_path.to_ascii_lowercase();
    let heading_lower = heading_catalog_text.to_ascii_lowercase();
    let document_search_lower = document_search_text.to_ascii_lowercase();
    let is_code_file = is_code_like_path(relative_path, file_name);

    let mut score = 0_i64;
    let mut matched_fields = Vec::new();

    for term in signal_terms {
        let term_lower = term.to_ascii_lowercase();
        if term_lower.is_empty() {
            continue;
        }

        if file_name_lower == term_lower || relative_path_lower == term_lower {
            score += 160;
            push_unique_field(&mut matched_fields, "exact_path");
            continue;
        }

        if file_name_lower.starts_with(&term_lower) {
            score += 80;
            push_unique_field(&mut matched_fields, "file_name");
        } else if file_name_lower.contains(&term_lower) {
            score += 55;
            push_unique_field(&mut matched_fields, "file_name");
        }

        if relative_path_lower.starts_with(&term_lower) {
            score += 65;
            push_unique_field(&mut matched_fields, "relative_path");
        } else if relative_path_lower.contains(&term_lower) {
            score += 45;
            push_unique_field(&mut matched_fields, "relative_path");
        }

        if heading_lower.contains(&term_lower) {
            score += 20;
            push_unique_field(&mut matched_fields, "heading_catalog");
        }

        if is_specific_document_signal_term(&term_lower)
            && document_search_lower.contains(&term_lower)
        {
            if is_code_file && looks_like_code_symbol_term(&term_lower) {
                score += 95;
                push_unique_field(&mut matched_fields, "exact_symbol");
            } else {
                score += if term_lower.chars().any(is_cjk_char)
                    || term_lower.chars().any(|ch| ch.is_ascii_digit())
                    || term_lower
                        .chars()
                        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
                {
                    40
                } else {
                    18
                };
                push_unique_field(&mut matched_fields, "document_search_text");
            }
        }
    }

    if score == 0 {
        None
    } else {
        Some((score, matched_fields))
    }
}

pub(crate) fn score_document_phrase_signal_match(
    phrase_terms: &[String],
    file_name: &str,
    relative_path: &str,
    heading_catalog_text: &str,
    document_search_text: &str,
) -> Option<(i64, Vec<String>, bool)> {
    let file_name_lower = file_name.to_ascii_lowercase();
    let relative_path_lower = relative_path.to_ascii_lowercase();
    let heading_lower = heading_catalog_text.to_ascii_lowercase();
    let document_search_lower = document_search_text.to_ascii_lowercase();

    let mut score = 0_i64;
    let mut matched_fields = Vec::new();
    let mut has_specific_match = false;

    for phrase in phrase_terms {
        let needle = phrase.trim().to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }
        let mut matched = false;

        if file_name_lower.contains(&needle) || relative_path_lower.contains(&needle) {
            score += 90;
            push_unique_field(&mut matched_fields, "docs_phrase");
            matched = true;
        }
        if heading_lower.contains(&needle) {
            score += 120;
            push_unique_field(&mut matched_fields, "docs_phrase");
            matched = true;
        }
        if document_search_lower.contains(&needle) {
            score += if needle.contains('/')
                || needle.contains('-')
                || needle.chars().any(is_cjk_char)
            {
                120
            } else {
                100
            };
            push_unique_field(&mut matched_fields, "docs_phrase");
            matched = true;
        }
        if matched && is_specific_phrase_signal_term(&needle) {
            has_specific_match = true;
        }
    }

    if score == 0 {
        None
    } else {
        Some((score, matched_fields, has_specific_match))
    }
}

pub(crate) fn is_specific_phrase_signal_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.chars().any(|ch| ch.is_ascii_digit())
        || trimmed
            .chars()
            .any(|ch| matches!(ch, '.' | '_' | '-' | '/' | '\\'))
    {
        return true;
    }
    if trimmed.chars().any(is_cjk_char) {
        return trimmed.chars().count() >= 4;
    }
    trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .count()
        >= 6
}

pub(crate) fn is_code_like_path(relative_path: &str, file_name: &str) -> bool {
    [relative_path, file_name].iter().any(|value| {
        value
            .rsplit_once('.')
            .map(|(_, ext)| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "rs" | "ts" | "tsx" | "js" | "jsx"
                )
            })
            .unwrap_or(false)
    })
}

pub(crate) fn looks_like_code_symbol_term(term: &str) -> bool {
    term.chars()
        .any(|ch| matches!(ch, '_' | '/' | '\\' | '.' | '-'))
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term.ends_with("_ms")
}

pub(crate) fn is_specific_document_signal_term(term: &str) -> bool {
    term.chars().any(is_cjk_char)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().count() >= 4
}

pub(crate) fn push_unique_field(fields: &mut Vec<String>, field: &str) {
    if !fields.iter().any(|item| item == field) {
        fields.push(field.to_string());
    }
}

pub(crate) fn push_unique_symbol(
    symbols: &mut Vec<String>,
    seen: &mut HashSet<String>,
    value: &str,
) {
    let normalized = value.trim();
    if normalized.is_empty() {
        return;
    }
    if seen.insert(normalized.to_string()) {
        symbols.push(normalized.to_string());
    }
}

pub(crate) fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
    )
}
