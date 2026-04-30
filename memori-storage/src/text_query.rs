use super::*;

pub(crate) fn build_fts_match_query(query: &str, mode: FtsQueryMode) -> Option<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for term in extract_fts_terms(query) {
        if seen.insert(term.clone()) {
            terms.push(format!("\"{}\"", term.replace('"', "\"\"")));
        }
    }

    if terms.is_empty() {
        None
    } else {
        Some(match mode {
            FtsQueryMode::BroadOr => terms.join(" OR "),
            FtsQueryMode::StrictAnd => terms.join(" AND "),
        })
    }
}

pub(crate) fn extract_fts_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    for token in extract_query_tokens(query) {
        for term in expand_query_token(&token) {
            let normalized = term.trim().to_string();
            if is_viable_search_term(&normalized) && seen.insert(normalized.clone()) {
                terms.push(normalized);
            }
        }
    }

    terms
}

pub(crate) fn extract_signal_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    for token in extract_query_tokens(query) {
        for term in expand_query_token(&token) {
            let normalized = normalize_ascii_term(&term);
            if is_viable_signal_term(&normalized) && seen.insert(normalized.clone()) {
                terms.push(normalized);
            }
        }
    }
    terms
}

pub(crate) fn extract_phrase_signal_terms(query: &str) -> Vec<String> {
    let filtered_tokens = extract_query_tokens(query)
        .into_iter()
        .flat_map(|token| expand_phrase_signal_token(&token))
        .filter(|token| is_viable_search_term(token) && !is_english_stopword(token))
        .collect::<Vec<_>>();

    let mut phrases = Vec::new();
    let mut seen = HashSet::new();
    let max_window = filtered_tokens.len().min(4);

    if filtered_tokens.len() == 1 {
        let token = filtered_tokens[0].clone();
        if is_viable_phrase_signal_term(&token) && seen.insert(token.clone()) {
            phrases.push(token);
        }
    }
    for token in &filtered_tokens {
        if !ends_with_cjk_question_suffix(token)
            && is_specific_phrase_signal_term(token)
            && is_viable_phrase_signal_term(token)
            && seen.insert(token.clone())
        {
            phrases.push(token.clone());
        }
    }

    for window in (2..=max_window).rev() {
        for slice in filtered_tokens.windows(window) {
            let phrase = slice.join(" ");
            if !ends_with_cjk_question_suffix(&phrase)
                && is_viable_phrase_signal_term(&phrase)
                && seen.insert(phrase.clone())
            {
                phrases.push(phrase);
            }
        }
    }

    if filtered_tokens.len() <= 6 {
        let full_phrase = filtered_tokens.join(" ");
        if !ends_with_cjk_question_suffix(&full_phrase)
            && is_viable_phrase_signal_term(&full_phrase)
            && seen.insert(full_phrase.clone())
        {
            phrases.push(full_phrase);
        }
    }

    phrases
}

pub(crate) fn expand_phrase_signal_token(token: &str) -> Vec<String> {
    let normalized = normalize_phrase_signal_token(token);
    let mut terms = Vec::new();
    let mut seen = HashSet::new();

    if !normalized.is_empty() && seen.insert(normalized.clone()) {
        terms.push(normalized);
    }

    if token.chars().any(is_cjk_char) {
        let cjk_with_digits = token
            .chars()
            .filter(|ch| is_cjk_char(*ch) || ch.is_ascii_digit())
            .collect::<String>();
        for phrase in extract_cjk_phrases(&cjk_with_digits) {
            if seen.insert(phrase.clone()) {
                terms.push(phrase);
            }
        }
    }

    terms
}

pub(crate) fn normalize_phrase_signal_token(token: &str) -> String {
    let normalized = normalize_ascii_term(token);
    if !normalized.chars().any(is_cjk_char) {
        return normalized;
    }
    let cjk_with_digits = normalized
        .chars()
        .filter(|ch| is_cjk_char(*ch) || ch.is_ascii_digit())
        .collect::<String>();
    if cjk_with_digits.is_empty() {
        return normalized;
    }
    let stripped = strip_cjk_question_tail(&cjk_with_digits);
    let compacted = compact_cjk_phrase(&stripped);
    if !compacted.is_empty() {
        compacted
    } else if !stripped.is_empty() {
        stripped
    } else {
        cjk_with_digits
    }
}

pub(crate) fn extract_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if is_query_term_char(ch) {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
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

    let mut results = Vec::new();
    let mut candidates = vec![trimmed.to_string()];
    candidates.extend(extract_mixed_script_segments(trimmed));

    for candidate in candidates {
        let normalized = normalize_ascii_term(&candidate);
        if !normalized.is_empty() {
            results.push(normalized.clone());
        }

        if candidate.chars().any(is_cjk_char) {
            let cjk_only = candidate
                .chars()
                .filter(|ch| is_cjk_char(*ch) || ch.is_ascii_digit())
                .collect::<String>();
            if !cjk_only.is_empty() {
                results.push(cjk_only.clone());
            }
            let pure_cjk = cjk_only
                .chars()
                .filter(|ch| is_cjk_char(*ch))
                .collect::<String>();
            if !pure_cjk.is_empty() {
                results.push(pure_cjk.clone());
            }
            for phrase in extract_cjk_phrases(&cjk_only) {
                results.push(phrase);
            }
            for phrase in extract_cjk_phrases(&pure_cjk) {
                results.push(phrase);
            }
        }

        if candidate
            .chars()
            .any(|ch| matches!(ch, '_' | '-' | '.' | '/' | '\\'))
        {
            if let Some((stem, _ext)) = candidate.rsplit_once('.') {
                let stem = normalize_ascii_term(stem);
                if !stem.is_empty() {
                    results.push(stem);
                }
            }
            for segment in candidate.split(['_', '-', '.', '/', '\\']) {
                let segment = normalize_ascii_term(segment);
                if !segment.is_empty() {
                    results.push(segment);
                }
            }
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for item in results {
        if seen.insert(item.clone()) {
            deduped.push(item);
        }
    }
    deduped
}

pub(crate) fn extract_cjk_phrases(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let stripped_tail = strip_cjk_question_tail(trimmed);
    let mut seeds = Vec::new();
    if !is_cjk_question_phrase(trimmed) {
        seeds.push(trimmed.to_string());
    }
    if !stripped_tail.is_empty() && !is_cjk_question_phrase(&stripped_tail) {
        seeds.push(stripped_tail.clone());
    }
    seeds.extend(extract_cjk_prefix_backoff_terms(&stripped_tail));
    seeds.extend(extract_cjk_prefix_backoff_terms(trimmed));

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for seed in seeds {
        for phrase in cjk_phrase_variants(&seed) {
            if is_viable_search_term(&phrase)
                && !is_cjk_question_phrase(&phrase)
                && seen.insert(phrase.clone())
            {
                deduped.push(phrase);
            }
        }
    }
    deduped
}

pub(crate) fn is_cjk_question_phrase(token: &str) -> bool {
    CJK_QUESTION_SUFFIXES.contains(&token) || CJK_QUESTION_SUFFIX_FALLBACKS.contains(&token)
}

pub(crate) fn extract_mixed_script_segments(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty()
        || !trimmed.chars().any(is_cjk_char)
        || !trimmed.chars().any(|ch| ch.is_ascii_alphanumeric())
    {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_is_cjk = None;

    for ch in trimmed.chars() {
        let ch_is_cjk = is_cjk_char(ch);
        match current_is_cjk {
            None => {
                current_is_cjk = Some(ch_is_cjk);
                current.push(ch);
            }
            Some(prev_is_cjk) if prev_is_cjk == ch_is_cjk => current.push(ch),
            Some(_) => {
                if current.chars().count() >= 2 {
                    segments.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(ch);
                current_is_cjk = Some(ch_is_cjk);
            }
        }
    }

    if current.chars().count() >= 2 {
        segments.push(current);
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for segment in segments {
        if segment != trimmed && seen.insert(segment.clone()) {
            deduped.push(segment);
        }
    }
    deduped
}

pub(crate) fn extract_cjk_prefix_backoff_terms(token: &str) -> Vec<String> {
    let chars = token.chars().collect::<Vec<_>>();
    if chars.len() < 5 {
        return Vec::new();
    }

    let min_len = if chars.len() <= 6 { 2 } else { 4 };
    let mut terms = Vec::new();
    let mut seen = HashSet::new();
    for len in (min_len..chars.len()).rev() {
        let candidate = chars[..len].iter().collect::<String>();
        if !is_cjk_question_phrase(&candidate) && seen.insert(candidate.clone()) {
            terms.push(candidate);
        }
    }
    terms
}

pub(crate) fn cjk_phrase_variants(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let stripped = strip_cjk_question_tail(trimmed);
    let compacted = compact_cjk_phrase(&stripped);
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    for candidate in [trimmed.to_string(), stripped, compacted] {
        if is_viable_search_term(&candidate)
            && !is_cjk_question_phrase(&candidate)
            && seen.insert(candidate.clone())
        {
            variants.push(candidate);
        }
    }
    for segment in split_cjk_phrase_on_fillers(trimmed) {
        if is_viable_search_term(&segment)
            && !is_cjk_question_phrase(&segment)
            && seen.insert(segment.clone())
        {
            variants.push(segment);
        }
    }
    let stripped_segments = split_cjk_phrase_on_fillers(&strip_cjk_question_tail(trimmed));
    if stripped_segments.len() >= 2 {
        let joined = stripped_segments.join("");
        if is_viable_search_term(&joined)
            && !is_cjk_question_phrase(&joined)
            && seen.insert(joined.clone())
        {
            variants.push(joined);
        }
    }

    variants
}

pub(crate) fn strip_cjk_question_tail(token: &str) -> String {
    let mut current = token.trim().to_string();
    if current.is_empty() {
        return current;
    }

    loop {
        let mut changed = false;
        let trimmed_edges = trim_cjk_filler_edges(&current);
        if trimmed_edges != current {
            current = trimmed_edges;
            changed = true;
        }

        if let Some(next) = strip_cjk_question_suffix_once(&current) {
            current = next;
            continue;
        }

        if !changed {
            break;
        }
    }

    current
}

pub(crate) fn strip_cjk_question_suffix_once(token: &str) -> Option<String> {
    for suffix in CJK_QUESTION_SUFFIXES {
        if token.ends_with(suffix) {
            return Some(token.trim_end_matches(suffix).trim().to_string());
        }
    }
    for suffix in CJK_QUESTION_SUFFIX_FALLBACKS {
        if token.ends_with(suffix) {
            return Some(token.trim_end_matches(suffix).trim().to_string());
        }
    }
    None
}

pub(crate) fn ends_with_cjk_question_suffix(token: &str) -> bool {
    let trimmed = token.trim();
    CJK_QUESTION_SUFFIXES
        .iter()
        .chain(CJK_QUESTION_SUFFIX_FALLBACKS.iter())
        .any(|suffix| trimmed.ends_with(suffix))
}

pub(crate) fn trim_cjk_filler_edges(token: &str) -> String {
    token
        .trim()
        .trim_matches(is_cjk_filler_char)
        .trim()
        .to_string()
}

pub(crate) fn compact_cjk_phrase(token: &str) -> String {
    token
        .chars()
        .filter(|ch| (is_cjk_char(*ch) || ch.is_ascii_digit()) && !is_cjk_filler_char(*ch))
        .collect::<String>()
}

pub(crate) fn split_cjk_phrase_on_fillers(token: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut seen = HashSet::new();

    for ch in token.chars() {
        if is_cjk_filler_char(ch) || !is_cjk_or_digit(ch) {
            if current.chars().count() >= 2 {
                let segment = std::mem::take(&mut current);
                if seen.insert(segment.clone()) {
                    segments.push(segment);
                }
            } else {
                current.clear();
            }
            continue;
        }
        current.push(ch);
    }

    if current.chars().count() >= 2 {
        let segment = std::mem::take(&mut current);
        if seen.insert(segment.clone()) {
            segments.push(segment);
        }
    }

    segments
}

pub(crate) fn is_cjk_or_digit(ch: char) -> bool {
    is_cjk_char(ch) || ch.is_ascii_digit()
}

pub(crate) fn is_cjk_filler_char(ch: char) -> bool {
    CJK_FILLER_CHARS.contains(&ch)
}

pub(crate) fn normalize_ascii_term(term: &str) -> String {
    term.trim()
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

pub(crate) fn is_viable_search_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk_char) {
        return trimmed.chars().count() >= 2;
    }
    if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return trimmed.chars().count() >= 1;
    }
    trimmed.chars().count() >= 2
}

pub(crate) fn is_viable_signal_term(term: &str) -> bool {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_english_stopword(trimmed) {
        return false;
    }
    if trimmed.chars().any(is_cjk_char) {
        return trimmed.chars().count() >= 2;
    }
    trimmed.chars().count() >= 2 || trimmed.chars().all(|ch| ch.is_ascii_digit())
}

pub(crate) fn is_viable_phrase_signal_term(term: &str) -> bool {
    let trimmed = term.trim();
    !trimmed.is_empty()
        && (trimmed.chars().count() >= 6
            || trimmed.chars().any(is_cjk_char)
            || trimmed
                .chars()
                .any(|ch| matches!(ch, '_' | '-' | '.' | '/' | '\\')))
}

pub(crate) fn is_query_term_char(ch: char) -> bool {
    is_cjk_char(ch) || ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\')
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

