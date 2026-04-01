use super::*;

const CJK_QUESTION_SUFFIXES: &[&str] = &[
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
];
const CJK_FILLER_CHARS: &[char] = &['的', '了', '吗', '呢', '啊', '吧', '呀', '嘛', '么', '是'];

pub(crate) fn normalize_scope_path_text(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut text = path.to_string_lossy().replace('/', "\\");

        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            text = stripped.to_string();
        } else if let Some(stripped) = text.strip_prefix(r"\??\") {
            text = stripped.to_string();
        }

        while text.len() > 3 && text.ends_with('\\') {
            text.pop();
        }

        text.to_ascii_lowercase()
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut text = path.to_string_lossy().to_string();
        while text.len() > 1 && text.ends_with('/') {
            text.pop();
        }
        text
    }
}

pub(crate) fn path_is_within_scope_dir(file_path: &str, scope_dir: &str) -> bool {
    if file_path == scope_dir {
        return true;
    }

    if scope_dir.is_empty() {
        return false;
    }

    #[cfg(target_os = "windows")]
    let sep = '\\';
    #[cfg(not(target_os = "windows"))]
    let sep = '/';

    let mut prefix = scope_dir.to_string();
    if !prefix.ends_with(sep) {
        prefix.push(sep);
    }

    file_path.starts_with(&prefix)
}

pub(crate) fn normalize_storage_file_path_text(path: &Path) -> String {
    normalize_scope_path_text(path)
}

pub(crate) fn build_relative_path(file_path: &Path, watch_root: Option<&Path>) -> String {
    if let Some(root) = watch_root
        && let Ok(relative) = file_path.strip_prefix(root)
    {
        let text = relative.to_string_lossy().to_string();
        if !text.trim().is_empty() {
            return normalize_relative_path_text(&text);
        }
    }

    file_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(normalize_relative_path_text)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| normalize_relative_path_text(file_path.to_string_lossy()))
}

pub(crate) fn normalize_relative_path_text(text: impl AsRef<str>) -> String {
    #[cfg(target_os = "windows")]
    {
        text.as_ref().replace('\\', "/")
    }

    #[cfg(not(target_os = "windows"))]
    {
        text.as_ref().to_string()
    }
}

pub(crate) fn inferred_file_size(chunks: &[DocumentChunk]) -> i64 {
    let byte_len: usize = chunks.iter().map(|chunk| chunk.content.len()).sum();
    i64::try_from(byte_len).unwrap_or(i64::MAX)
}

pub(crate) fn build_heading_catalog_text(chunks: &[DocumentChunk]) -> String {
    let mut seen = HashSet::new();
    let mut headings = Vec::new();

    for chunk in chunks {
        if chunk.heading_path.is_empty() {
            continue;
        }
        let joined = chunk.heading_path.join(" / ");
        if seen.insert(joined.clone()) {
            headings.push(joined);
        }
    }

    headings.join("\n")
}

pub(crate) fn build_document_search_text(
    catalog_entry: &CatalogEntry,
    heading_catalog_text: &str,
    chunks: &[DocumentChunk],
) -> String {
    let mut sections = Vec::new();

    if !catalog_entry.file_name.trim().is_empty() {
        sections.push(catalog_entry.file_name.trim().to_string());
    }
    if !catalog_entry.relative_path.trim().is_empty()
        && catalog_entry.relative_path != catalog_entry.file_name
    {
        sections.push(catalog_entry.relative_path.trim().to_string());
    }
    if !heading_catalog_text.trim().is_empty() {
        sections.push(heading_catalog_text.trim().to_string());
    }

    let code_symbol_text = build_code_symbol_text(catalog_entry, chunks);
    if !code_symbol_text.trim().is_empty() {
        sections.push(code_symbol_text);
    }

    let preview = build_document_preview_text(chunks);
    if !preview.trim().is_empty() {
        sections.push(preview.trim().to_string());
    }

    sections.join("\n")
}

pub(crate) fn build_document_preview_text(chunks: &[DocumentChunk]) -> String {
    let mut snippets = Vec::new();
    let mut seen_snippets = HashSet::new();
    for chunk in chunks {
        let snippet = chunk_preview_snippet(&chunk.content);
        if snippet.is_empty() || !seen_snippets.insert(snippet.clone()) {
            continue;
        }
        snippets.push(snippet);
    }

    if snippets.is_empty() {
        return String::new();
    }

    let sample_count = snippets.len().min(DOCUMENT_SEARCH_MAX_SNIPPETS);
    let sampled_indices = evenly_sample_indices(snippets.len(), sample_count);
    let mut preview = String::new();

    for index in sampled_indices {
        let Some(snippet) = snippets.get(index) else {
            continue;
        };
        let candidate_len =
            preview.chars().count() + snippet.chars().count() + usize::from(!preview.is_empty());
        if candidate_len > DOCUMENT_SEARCH_PREVIEW_CHARS {
            break;
        }

        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(snippet);
    }

    preview
}

pub(crate) fn evenly_sample_indices(total: usize, count: usize) -> Vec<usize> {
    if total == 0 || count == 0 {
        return Vec::new();
    }
    if count >= total {
        return (0..total).collect();
    }

    let max_index = total - 1;
    let denominator = count - 1;
    let mut indices = Vec::with_capacity(count);
    for position in 0..count {
        let index = if denominator == 0 {
            0
        } else {
            (position * max_index + denominator / 2) / denominator
        };
        if indices.last().copied() != Some(index) {
            indices.push(index);
        }
    }
    indices
}

pub(crate) fn build_code_symbol_text(
    catalog_entry: &CatalogEntry,
    chunks: &[DocumentChunk],
) -> String {
    if !is_code_like_document(catalog_entry) {
        return String::new();
    }

    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    for chunk in chunks {
        extract_code_symbol_terms(&chunk.content, &mut symbols, &mut seen);
    }

    let mut rendered = String::new();
    for symbol in symbols {
        let candidate_len =
            rendered.chars().count() + symbol.chars().count() + usize::from(!rendered.is_empty());
        if candidate_len > DOCUMENT_SEARCH_SYMBOL_CHARS {
            break;
        }
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&symbol);
    }

    rendered
}

pub(crate) fn is_code_like_document(catalog_entry: &CatalogEntry) -> bool {
    matches!(
        catalog_entry
            .file_ext
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx"
    )
}

pub(crate) fn extract_code_symbol_terms(
    content: &str,
    symbols: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        for keyword in ["fn", "struct", "enum", "type", "trait", "impl", "mod"] {
            if let Some(identifier) = extract_identifier_after_keyword(trimmed, keyword) {
                push_unique_symbol(symbols, seen, &identifier);
            }
        }

        if let Some(identifier) = extract_field_name(trimmed) {
            push_unique_symbol(symbols, seen, &identifier);
        }

        for qualified in extract_qualified_identifiers(trimmed) {
            push_unique_symbol(symbols, seen, &qualified);
        }

        for literal in extract_interesting_string_literals(trimmed) {
            push_unique_symbol(symbols, seen, &literal);
        }
    }
}

pub(crate) fn extract_identifier_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let keyword_bytes = keyword.as_bytes();
    let mut index = 0;

    while index + keyword_bytes.len() <= bytes.len() {
        if &bytes[index..index + keyword_bytes.len()] == keyword_bytes {
            let before = index
                .checked_sub(1)
                .and_then(|pos| bytes.get(pos))
                .copied()
                .map(char::from);
            let after = bytes
                .get(index + keyword_bytes.len())
                .copied()
                .map(char::from);
            let before_is_ident = before.is_some_and(is_identifier_char);
            let after_is_boundary = after.is_none_or(|ch| ch.is_whitespace() || ch == '<');
            if !before_is_ident && after_is_boundary {
                let rest = line[index + keyword_bytes.len()..].trim_start();
                let identifier = take_identifier(rest);
                if is_symbol_identifier(&identifier) {
                    return Some(identifier);
                }
            }
        }
        index += 1;
    }

    None
}

pub(crate) fn extract_field_name(line: &str) -> Option<String> {
    let mut rest = line.trim_start();
    for prefix in ["pub(crate) ", "pub(super) ", "pub(self) ", "pub "] {
        if let Some(stripped) = rest.strip_prefix(prefix) {
            rest = stripped.trim_start();
            break;
        }
    }

    let identifier = take_identifier(rest);
    if !is_symbol_identifier(&identifier) {
        return None;
    }

    let remaining = rest[identifier.len()..].trim_start();
    if remaining.starts_with(':') {
        Some(identifier)
    } else {
        None
    }
}

pub(crate) fn extract_interesting_string_literals(line: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }

            match ch {
                '\\' => escaped = true,
                '"' => {
                    if is_interesting_symbol_literal(&current) {
                        literals.push(current.trim().to_string());
                    }
                    current.clear();
                    in_string = false;
                }
                _ => current.push(ch),
            }
        } else if ch == '"' {
            in_string = true;
        }
    }

    literals
}

pub(crate) fn extract_qualified_identifiers(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':') {
            current.push(ch);
            continue;
        }

        maybe_push_qualified_identifier(&mut values, &mut current);
    }
    maybe_push_qualified_identifier(&mut values, &mut current);

    values
}

pub(crate) fn maybe_push_qualified_identifier(target: &mut Vec<String>, current: &mut String) {
    let candidate = current
        .trim_matches(|ch: char| matches!(ch, '.' | ':'))
        .to_string();
    current.clear();

    if candidate.is_empty() || (!candidate.contains('.') && !candidate.contains("::")) {
        return;
    }

    let normalized = candidate.replace("::", ".");
    let valid = normalized
        .split('.')
        .filter(|segment| !segment.is_empty())
        .all(is_symbol_identifier);
    if valid && normalized.chars().count() >= 8 {
        target.push(normalized);
    }
}

pub(crate) fn is_interesting_symbol_literal(literal: &str) -> bool {
    let trimmed = literal.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.contains('/')
        || trimmed.contains('_')
        || trimmed.contains('.')
        || trimmed.contains('-')
        || trimmed.contains("::")
        || (trimmed.chars().count() >= 6 && trimmed.chars().any(|ch| ch.is_ascii_alphabetic()))
}

pub(crate) fn take_identifier(text: &str) -> String {
    text.chars()
        .take_while(|ch| is_identifier_char(*ch))
        .collect::<String>()
}

pub(crate) fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

pub(crate) fn is_symbol_identifier(identifier: &str) -> bool {
    let trimmed = identifier.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
}

pub(crate) fn chunk_preview_snippet(content: &str) -> String {
    let collapsed = content
        .split_whitespace()
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    trimmed
        .chars()
        .take(DOCUMENT_SEARCH_SNIPPET_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

pub(crate) enum FtsQueryMode {
    BroadOr,
    StrictAnd,
}

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
        .map(|token| normalize_phrase_signal_token(&token))
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

    for window in (2..=max_window).rev() {
        for slice in filtered_tokens.windows(window) {
            let phrase = slice.join(" ");
            if is_viable_phrase_signal_term(&phrase) && seen.insert(phrase.clone()) {
                phrases.push(phrase);
            }
        }
    }

    if filtered_tokens.len() <= 6 {
        let full_phrase = filtered_tokens.join(" ");
        if is_viable_phrase_signal_term(&full_phrase) && seen.insert(full_phrase.clone()) {
            phrases.push(full_phrase);
        }
    }

    phrases
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
    CJK_QUESTION_SUFFIXES.contains(&token)
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
    None
}

pub(crate) fn trim_cjk_filler_edges(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch| is_cjk_filler_char(ch))
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
