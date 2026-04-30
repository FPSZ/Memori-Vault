use super::*;

pub(crate) fn is_cjk_or_digit(ch: char) -> bool {
    is_cjk(ch) || ch.is_ascii_digit()
}

pub(crate) fn is_cjk_filler_char(ch: char) -> bool {
    CJK_FILLER_CHARS.contains(&ch)
}

pub(crate) fn is_direct_lexical_support_like_term(term: &str) -> bool {
    term.chars().any(is_cjk)
        || term.chars().any(|ch| ch.is_ascii_digit())
        || term
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        || term.chars().count() >= 6
}

pub(crate) fn is_cjk_question_phrase(token: &str) -> bool {
    CJK_QUESTION_SUFFIXES.contains(&token)
}

pub(crate) fn extract_mixed_script_segments(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty()
        || !trimmed.chars().any(is_cjk)
        || !trimmed.chars().any(|ch| ch.is_ascii_alphanumeric())
    {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_is_cjk = None;

    for ch in trimmed.chars() {
        let ch_is_cjk = is_cjk(ch);
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
    let mut seen = HashMap::<String, ()>::new();
    for segment in segments {
        if segment != trimmed {
            insert_unique_term(&mut seen, &mut deduped, &segment);
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
    let mut seen = HashMap::<String, ()>::new();
    for len in (min_len..chars.len()).rev() {
        let candidate = chars[..len].iter().collect::<String>();
        if !is_cjk_question_phrase(&candidate) {
            insert_unique_term(&mut seen, &mut terms, &candidate);
        }
    }
    terms
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
        .map(|token| normalize_docs_phrase_token(token))
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

pub(crate) fn normalize_docs_phrase_token(token: &str) -> String {
    let normalized = normalize_ascii_token(token);
    if !normalized.chars().any(is_cjk) {
        return normalized;
    }
    let cjk_with_digits = normalized
        .chars()
        .filter(|ch| is_cjk(*ch) || ch.is_ascii_digit())
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

pub(crate) fn detect_compound_query(query: &str) -> Option<CompoundQueryPlan> {
    let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() < 6 {
        return None;
    }
    let lower = normalized.to_ascii_lowercase();
    let has_compound_marker = normalized.contains("分别")
        || normalized.contains("对比")
        || normalized.contains("比较")
        || normalized.contains("各自")
        || normalized.contains("和")
        || normalized.contains("与")
        || normalized.contains("、")
        || normalized.contains('/')
        || lower.contains(" and ")
        || lower.contains(" vs ")
        || lower.contains(" versus ")
        || lower.contains(" compare ");
    if !has_compound_marker {
        return None;
    }

    let raw_tokens = extract_query_tokens(&normalized);
    if raw_tokens.len() > 32 {
        return None;
    }
    let mut topics = extract_compound_topics_from_text(&normalized);
    if topics.len() < 2 && raw_tokens.len() >= 2 {
        topics = extract_compound_topics(&raw_tokens);
    }
    if topics.len() < 2 {
        return None;
    }
    let focus = build_compound_focus(&normalized, &topics);
    let parts = topics
        .into_iter()
        .take(4)
        .map(|topic| {
            let query = if focus.is_empty() {
                topic.clone()
            } else {
                format!("{topic} {focus}")
            };
            CompoundQueryPart { topic, query }
        })
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        None
    } else {
        Some(CompoundQueryPlan { parts })
    }
}

fn extract_compound_topics(raw_tokens: &[String]) -> Vec<String> {
    let mut topics = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for token in raw_tokens {
        let trimmed = trim_compound_topic_token(token);
        if trimmed.is_empty() || is_compound_connector_or_question(trimmed) {
            continue;
        }
        let expanded = expand_query_token(trimmed);
        let has_specific_signal = trimmed.chars().any(|ch| ch.is_ascii_digit())
            || trimmed
                .chars()
                .any(|ch| matches!(ch, '-' | '_' | '.' | '/' | '\\'))
            || expanded.iter().any(|term| {
                term.chars().any(|ch| ch.is_ascii_digit())
                    || term
                        .chars()
                        .any(|ch| matches!(ch, '-' | '_' | '.' | '/' | '\\'))
            })
            || is_specific_cjk_topic(trimmed);
        if !has_specific_signal {
            continue;
        }
        let normalized = trimmed.to_string();
        let key = normalized.to_ascii_lowercase();
        if seen.insert(key, ()).is_none() {
            topics.push(normalized);
        }
    }

    topics
}

fn extract_compound_topics_from_text(query: &str) -> Vec<String> {
    let mut topics = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    let normalized = query
        .replace("以及", "和")
        .replace("还有", "和")
        .replace("与", "和")
        .replace('、', "和")
        .replace(" and ", "和")
        .replace(" vs ", "和")
        .replace(" versus ", "和");

    for segment in normalized.split('和') {
        let candidate = normalize_compound_topic_segment(segment);
        if candidate.is_empty() || is_compound_connector_or_question(&candidate) {
            continue;
        }
        if !is_specific_compound_topic(&candidate) {
            continue;
        }
        let key = candidate.to_ascii_lowercase();
        if seen.insert(key, ()).is_none() {
            topics.push(candidate);
        }
    }

    topics
}

fn normalize_compound_topic_segment(segment: &str) -> String {
    let mut candidate = trim_compound_topic_token(segment).trim().to_string();
    for prefix in ["请对比", "对比", "比较", "请比较", "请问", "查询", "看看"] {
        if let Some(stripped) = candidate.strip_prefix(prefix) {
            candidate = stripped.trim().to_string();
        }
    }
    for marker in [
        "的负责人",
        "负责人",
        "的核心",
        "核心",
        "的关键",
        "关键",
        "的当前",
        "当前",
        "的风险",
        "风险",
        "的验收",
        "验收",
        "分别",
        "是谁",
        "是什么",
        "如何",
        "怎么",
    ] {
        if let Some((topic, _)) = candidate.split_once(marker) {
            candidate = topic.trim().to_string();
        }
    }
    trim_compound_topic_token(&candidate).to_string()
}

fn is_specific_cjk_topic(token: &str) -> bool {
    let cjk_count = token.chars().filter(|ch| is_cjk(*ch)).count();
    cjk_count >= 3
        && cjk_count <= 10
        && !CJK_DOC_NOISE_TERMS.contains(&token)
        && ![
            "负责人",
            "当前风险",
            "验收要求",
            "内部规定",
            "核心事实",
            "关键指标",
            "分别是谁",
            "是什么",
        ]
        .contains(&token)
}

fn is_specific_compound_topic(token: &str) -> bool {
    let expanded = expand_query_token(token);
    token.chars().any(|ch| ch.is_ascii_digit())
        || token
            .chars()
            .any(|ch| matches!(ch, '-' | '_' | '.' | '/' | '\\'))
        || expanded.iter().any(|term| {
            term.chars().any(|ch| ch.is_ascii_digit())
                || term
                    .chars()
                    .any(|ch| matches!(ch, '-' | '_' | '.' | '/' | '\\'))
        })
        || is_specific_cjk_topic(token)
}

fn trim_compound_topic_token(token: &str) -> &str {
    token.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '，' | ','
                    | '。'
                    | '.'
                    | '？'
                    | '?'
                    | '！'
                    | '!'
                    | '：'
                    | ':'
                    | '；'
                    | ';'
                    | '（'
                    | '('
                    | '）'
                    | ')'
                    | '【'
                    | '['
                    | '】'
                    | ']'
                    | '《'
                    | '<'
                    | '》'
                    | '>'
            )
    })
}

fn is_compound_connector_or_question(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "and" | "or" | "vs" | "versus" | "compare" | "comparison" | "project" | "projects"
    ) || matches!(
        token,
        "和" | "与" | "及" | "或" | "分别" | "对比" | "比较" | "各自" | "项目" | "内容" | "资料"
    ) || CJK_QUESTION_SUFFIXES
        .iter()
        .any(|suffix| token.contains(suffix))
}

fn build_compound_focus(query: &str, topics: &[String]) -> String {
    let mut focus = query.to_string();
    for topic in topics {
        focus = focus.replace(topic, " ");
    }
    for marker in [
        "分别", "对比", "比较", "各自", "以及", "还有", "和", "与", "、", "/", "，", ",", "？", "?",
    ] {
        focus = focus.replace(marker, " ");
    }
    for marker in [" and ", " vs ", " versus ", " compare ", " comparison "] {
        focus = focus.replace(marker, " ");
    }
    focus.split_whitespace().collect::<Vec<_>>().join(" ")
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
