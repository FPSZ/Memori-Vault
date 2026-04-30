use super::*;

pub(crate) const CJK_QUESTION_SUFFIXES: &[&str] = &[
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
pub(crate) const CJK_FILLER_CHARS: &[char] = &['的', '了', '吗', '呢', '啊', '吧', '呀', '嘛', '么', '是'];

pub(crate) const CJK_QUESTION_SUFFIX_FALLBACKS: &[&str] = &[
    "\u{662f}\u{4ec0}\u{4e48}",
    "\u{662f}\u{5565}",
    "\u{4ec0}\u{4e48}",
    "\u{600e}\u{4e48}",
    "\u{5982}\u{4f55}",
    "\u{4e3a}\u{4ec0}\u{4e48}",
    "\u{591a}\u{5c11}",
    "\u{54ea}\u{4e00}\u{4e2a}",
    "\u{54ea}\u{4e2a}",
    "\u{54ea}\u{91cc}",
    "\u{5728}\u{54ea}",
    "\u{8c01}",
];

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
        let index = (position * max_index + denominator / 2)
            .checked_div(denominator)
            .unwrap_or(0);
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
