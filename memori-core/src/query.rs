use super::*;

pub(crate) fn analyze_query(query: &str) -> QueryAnalysis {
    let normalized_query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    let raw_tokens = extract_query_tokens(&normalized_query);
    let mut lexical_terms = Vec::new();
    let mut document_terms = Vec::new();
    let mut filename_terms = Vec::new();
    let mut identifier_terms = Vec::new();
    let mut seen_lexical = HashMap::<String, ()>::new();
    let mut seen_document = HashMap::<String, ()>::new();
    let mut seen_filename = HashMap::<String, ()>::new();
    let mut seen_identifier = HashMap::<String, ()>::new();
    let mut flags = QueryFlags::default();

    for token in &raw_tokens {
        if token.chars().any(is_cjk) {
            flags.has_cjk = true;
        }
        if token.chars().any(|ch| ch.is_ascii_alphabetic())
            && token
                .chars()
                .any(|ch| ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.' | '/' | '\\'))
        {
            flags.has_ascii_identifier = true;
        }
        if token
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        {
            flags.has_path_like_token = true;
        }

        for term in expand_query_token(token) {
            if is_english_stopword(&term) {
                continue;
            }
            if insert_unique_term(&mut seen_lexical, &mut lexical_terms, &term) {
                insert_unique_term(&mut seen_document, &mut document_terms, &term);
                if term.chars().any(|ch| ch.is_ascii_digit())
                    || term
                        .chars()
                        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
                {
                    insert_unique_term(&mut seen_filename, &mut filename_terms, &term);
                }
                if looks_like_identifier_term(&term, token) {
                    insert_unique_term(&mut seen_identifier, &mut identifier_terms, &term);
                }
            }
        }
    }

    if filename_terms.is_empty() {
        for term in &document_terms {
            if term.chars().any(is_cjk) || term.chars().any(|ch| ch.is_ascii_digit()) {
                insert_unique_term(&mut seen_filename, &mut filename_terms, term);
            }
        }
    }

    flags.token_count = lexical_terms.len();
    flags.is_lookup_like = is_lookup_like_query(&normalized_query)
        || flags.has_path_like_token
        || !filename_terms.is_empty()
        || !identifier_terms.is_empty();
    let query_intent = classify_query_intent(&normalized_query, &flags);
    let query_family = classify_query_family(
        &normalized_query,
        &raw_tokens,
        &document_terms,
        &filename_terms,
        &identifier_terms,
        &flags,
    );
    let docs_phrase_terms = extract_docs_phrase_terms(&raw_tokens, &document_terms, query_family);

    QueryAnalysis {
        normalized_query: normalized_query.clone(),
        lexical_query: query_string_for_terms(&lexical_terms, &normalized_query),
        document_routing_terms: if document_terms.is_empty() {
            lexical_terms.clone()
        } else {
            document_terms
        },
        docs_phrase_terms,
        chunk_terms: lexical_terms,
        filename_like_terms: filename_terms,
        identifier_terms,
        query_intent,
        query_family,
        flags,
    }
}

pub(crate) fn extract_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if is_query_token_char(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

pub(crate) fn expand_query_token(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut expanded = Vec::new();
    let normalized = normalize_ascii_token(trimmed);
    if is_valid_query_term(&normalized) {
        expanded.push(normalized.clone());
    }

    if trimmed.chars().any(is_cjk) {
        let cjk_with_digits = trimmed
            .chars()
            .filter(|ch| is_cjk(*ch) || ch.is_ascii_digit())
            .collect::<String>();
        if is_valid_query_term(&cjk_with_digits) {
            expanded.push(cjk_with_digits.clone());
        }
        let pure_cjk = cjk_with_digits
            .chars()
            .filter(|ch| is_cjk(*ch))
            .collect::<String>();
        if is_valid_query_term(&pure_cjk) {
            expanded.push(pure_cjk.clone());
        }
        for phrase in extract_cjk_query_phrases(&cjk_with_digits) {
            if is_valid_query_term(&phrase) {
                expanded.push(phrase);
            }
        }
        for phrase in extract_cjk_query_phrases(&pure_cjk) {
            if is_valid_query_term(&phrase) {
                expanded.push(phrase);
            }
        }
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
    {
        if let Some((stem, _ext)) = trimmed.rsplit_once('.') {
            let stem = normalize_ascii_token(stem);
            if is_valid_query_term(&stem) {
                expanded.push(stem);
            }
        }
        for part in trimmed.split(['.', '/', '\\', '_', '-']) {
            let normalized = normalize_ascii_token(part);
            if is_valid_query_term(&normalized) {
                expanded.push(normalized);
            }
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    for term in expanded {
        insert_unique_term(&mut seen, &mut deduped, &term);
    }
    deduped
}

pub(crate) fn extract_cjk_query_phrases(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut phrases = Vec::new();
    if !is_cjk_question_phrase(trimmed) {
        phrases.push(trimmed.to_string());
    }

    for suffix in [
        "是什么",
        "是啥",
        "什么",
        "怎么",
        "如何",
        "为什么",
        "多少",
        "哪一个",
        "哪个",
        "哪里",
        "在哪",
        "谁",
    ] {
        if trimmed.ends_with(suffix) {
            let candidate = trimmed.trim_end_matches(suffix).trim().to_string();
            if candidate.chars().count() >= 2 {
                phrases.push(candidate);
            }
        }
    }

    phrases
}

pub(crate) fn is_cjk_question_phrase(token: &str) -> bool {
    matches!(
        token,
        "是什么"
            | "是啥"
            | "什么"
            | "怎么"
            | "如何"
            | "为什么"
            | "多少"
            | "哪一个"
            | "哪个"
            | "哪里"
            | "在哪"
            | "谁"
    )
}

pub(crate) fn normalize_ascii_token(token: &str) -> String {
    token
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect::<String>()
}

pub(crate) fn is_valid_query_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk) {
        return trimmed.chars().count() >= 2;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return !trimmed.is_empty();
    }
    trimmed.chars().count() >= 2
}

pub(crate) fn is_query_token_char(ch: char) -> bool {
    is_cjk(ch) || ch.is_ascii_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '_' | '-')
}

pub(crate) fn looks_like_identifier_term(term: &str, raw_token: &str) -> bool {
    term.chars()
        .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().any(|ch| ch.is_ascii_digit())
        || raw_token
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || raw_token.chars().any(|ch| ch.is_ascii_digit())
        || has_ascii_camel_case(raw_token)
}

pub(crate) fn extract_docs_phrase_terms(
    raw_tokens: &[String],
    document_terms: &[String],
    query_family: QueryFamily,
) -> Vec<String> {
    if matches!(query_family, QueryFamily::ImplementationLookup) {
        return Vec::new();
    }

    let filtered_tokens = raw_tokens
        .iter()
        .map(|token| normalize_ascii_token(token))
        .filter(|token| is_valid_query_term(token) && !is_english_stopword(token))
        .collect::<Vec<_>>();

    let mut phrases = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for term in document_terms {
        if term.chars().count() >= 6
            || term.chars().any(is_cjk)
            || term
                .chars()
                .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        {
            insert_unique_term(&mut seen, &mut phrases, term);
        }
    }

    if filtered_tokens.is_empty() {
        return phrases;
    }

    let max_window = filtered_tokens.len().min(4);
    for window in (2..=max_window).rev() {
        for slice in filtered_tokens.windows(window) {
            let phrase = slice.join(" ");
            if phrase.chars().count() >= 6 {
                insert_unique_term(&mut seen, &mut phrases, &phrase);
            }
        }
    }

    if filtered_tokens.len() <= 6 {
        let full_phrase = filtered_tokens.join(" ");
        if full_phrase.chars().count() >= 6 {
            insert_unique_term(&mut seen, &mut phrases, &full_phrase);
        }
    }

    phrases
}

pub(crate) fn has_ascii_camel_case(token: &str) -> bool {
    let chars = token.chars().collect::<Vec<_>>();
    chars.windows(2).any(|pair| {
        let [left, right] = pair else {
            return false;
        };
        left.is_ascii_lowercase() && right.is_ascii_uppercase()
    })
}

pub(crate) fn insert_unique_term(
    seen: &mut HashMap<String, ()>,
    target: &mut Vec<String>,
    term: &str,
) -> bool {
    let normalized = term.trim().to_string();
    if normalized.is_empty() || seen.contains_key(&normalized) {
        return false;
    }
    seen.insert(normalized.clone(), ());
    target.push(normalized);
    true
}

pub(crate) fn query_string_for_terms(terms: &[String], fallback: &str) -> String {
    if terms.is_empty() {
        fallback.trim().to_string()
    } else {
        terms.join(" ")
    }
}

pub(crate) fn is_english_stopword(term: &str) -> bool {
    matches!(
        term.trim().to_ascii_lowercase().as_str(),
        "a" | "an"
            | "the"
            | "is"
            | "are"
            | "was"
            | "were"
            | "what"
            | "which"
            | "who"
            | "when"
            | "where"
            | "why"
            | "how"
            | "do"
            | "does"
            | "did"
            | "can"
            | "could"
            | "should"
            | "would"
            | "will"
            | "to"
            | "of"
            | "in"
            | "on"
            | "for"
            | "from"
            | "by"
            | "with"
            | "and"
            | "or"
            | "my"
            | "your"
            | "me"
    )
}

pub(crate) fn query_flags_as_labels(analysis: &QueryAnalysis) -> Vec<String> {
    let mut labels = Vec::new();
    let flags = &analysis.flags;
    if flags.has_cjk {
        labels.push("cjk".to_string());
    }
    if flags.has_ascii_identifier {
        labels.push("ascii_identifier".to_string());
    }
    if flags.has_path_like_token {
        labels.push("path_like".to_string());
    }
    if flags.is_lookup_like {
        labels.push("lookup_like".to_string());
    }
    labels.push(format!("query_family:{}", analysis.query_family.as_str()));
    labels.push(format!("token_count:{}", flags.token_count));
    labels
}

pub(crate) fn document_signal_query(analysis: &QueryAnalysis) -> String {
    let mut signal_terms = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for term in &analysis.identifier_terms {
        insert_unique_term(&mut seen, &mut signal_terms, term);
    }
    for term in &analysis.filename_like_terms {
        insert_unique_term(&mut seen, &mut signal_terms, term);
    }
    for term in &analysis.document_routing_terms {
        let should_include = matches!(analysis.query_family, QueryFamily::ImplementationLookup)
            || term.chars().any(is_cjk)
            || term.chars().any(|ch| ch.is_ascii_digit())
            || term
                .chars()
                .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
            || term.chars().count() >= 5;
        if should_include {
            insert_unique_term(&mut seen, &mut signal_terms, term);
        }
    }

    if signal_terms.is_empty() {
        analysis.normalized_query.clone()
    } else {
        signal_terms.join(" ")
    }
}

pub(crate) fn doc_top_k_for_query_family(query_family: QueryFamily) -> usize {
    match query_family {
        QueryFamily::DocsExplanatory | QueryFamily::DocsApiLookup => 16,
        QueryFamily::ImplementationLookup => DEFAULT_DOC_TOP_K,
    }
}
