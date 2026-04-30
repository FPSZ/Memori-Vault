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

/// 中文技术文档高频噪声词：这些词在几乎所有文档中都会出现，
/// 如果作为 strict lexical hit 会严重污染 rankings。
/// 它们在 support_terms 中被过滤，在 direct_chunk_lexical_signal 中只算 broad hit。
pub(crate) const CJK_DOC_NOISE_TERMS: &[&str] = &[
    "新增", "添加", "更新", "修改", "删除", "移除", "功能", "特性", "支持", "使用", "启用", "禁用",
    "配置", "设置", "方法", "函数", "文件", "说明", "描述", "介绍", "文档", "问题", "错误", "修复",
    "解决", "优化", "实现", "创建", "生成", "构建", "数据", "信息", "内容", "用户", "系统", "应用",
    "程序", "项目", "代码", "需要", "可以", "通过", "进行", "确保", "验证", "检查", "测试", "默认",
    "自动", "手动", "主要", "重要", "关键", "核心", "基本", "标准", "规范", "规则", "步骤", "过程",
    "结果", "相关", "包含", "包括", "基于", "根据", "按照",
];

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
            if term.chars().any(|ch| ch.is_ascii_digit()) {
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
    let support_terms = extract_query_support_terms(&raw_tokens, &lexical_terms);

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
        support_terms,
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
    let mut candidates = vec![trimmed.to_string()];
    candidates.extend(extract_mixed_script_segments(trimmed));

    for candidate in candidates {
        let normalized = normalize_ascii_token(&candidate);
        if is_valid_query_term(&normalized) {
            expanded.push(normalized.clone());
        }

        if candidate.chars().any(is_cjk) {
            let cjk_with_digits = candidate
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

        if candidate
            .chars()
            .any(|ch| matches!(ch, '.' | '/' | '\\' | '_' | '-'))
        {
            if let Some((stem, _ext)) = candidate.rsplit_once('.') {
                let stem = normalize_ascii_token(stem);
                if is_valid_query_term(&stem) {
                    expanded.push(stem);
                }
            }
            for part in candidate.split(['.', '/', '\\', '_', '-']) {
                let normalized = normalize_ascii_token(part);
                if is_valid_query_term(&normalized) {
                    expanded.push(normalized);
                }
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

    let mut phrases = Vec::new();
    let mut seen = HashMap::<String, ()>::new();
    for seed in seeds {
        for phrase in cjk_phrase_variants(&seed) {
            if is_valid_query_term(&phrase) && !is_cjk_question_phrase(&phrase) {
                insert_unique_term(&mut seen, &mut phrases, &phrase);
            }
        }
    }
    phrases
}

pub(crate) fn extract_query_support_terms(
    raw_tokens: &[String],
    lexical_terms: &[String],
) -> Vec<String> {
    let mut support_terms = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for term in lexical_terms {
        if term.chars().any(is_cjk) {
            continue;
        }
        if is_direct_lexical_support_like_term(term) {
            insert_unique_term(&mut seen, &mut support_terms, term);
        }
    }

    let mut cjk_candidates = Vec::<String>::new();
    for token in raw_tokens {
        if !token.chars().any(is_cjk) {
            continue;
        }
        let cjk_with_digits = token
            .chars()
            .filter(|ch| is_cjk(*ch) || ch.is_ascii_digit())
            .collect::<String>();
        if cjk_with_digits.is_empty() {
            continue;
        }
        for term in cjk_phrase_variants(&cjk_with_digits) {
            let normalized = strip_cjk_question_tail(&term);
            if normalized != term {
                continue;
            }
            if normalized.chars().any(is_cjk) && normalized.chars().any(is_cjk_filler_char) {
                continue;
            }
            if is_direct_lexical_support_like_term(&normalized) {
                cjk_candidates.push(normalized);
            }
        }
    }

    let has_specific_cjk_candidate = cjk_candidates
        .iter()
        .any(|term| term.chars().any(is_cjk) && !CJK_DOC_NOISE_TERMS.contains(&term.as_str()));
    for term in cjk_candidates {
        let is_noise = CJK_DOC_NOISE_TERMS.contains(&term.as_str());
        if is_noise && !has_specific_cjk_candidate {
            continue;
        }
        insert_unique_term(&mut seen, &mut support_terms, &term);
    }

    support_terms
}

pub(crate) fn cjk_phrase_variants(token: &str) -> Vec<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let stripped = strip_cjk_question_tail(trimmed);
    let compacted = compact_cjk_phrase(&stripped);
    let mut variants = Vec::new();
    let mut seen = HashMap::<String, ()>::new();

    for candidate in [trimmed.to_string(), stripped, compacted] {
        if is_valid_query_term(&candidate) && !is_cjk_question_phrase(&candidate) {
            insert_unique_term(&mut seen, &mut variants, &candidate);
        }
    }
    for segment in split_cjk_phrase_on_fillers(trimmed) {
        if is_valid_query_term(&segment) && !is_cjk_question_phrase(&segment) {
            insert_unique_term(&mut seen, &mut variants, &segment);
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
        .trim_matches(is_cjk_filler_char)
        .trim()
        .to_string()
}

pub(crate) fn compact_cjk_phrase(token: &str) -> String {
    token
        .chars()
        .filter(|ch| (is_cjk(*ch) || ch.is_ascii_digit()) && !is_cjk_filler_char(*ch))
        .collect::<String>()
}

pub(crate) fn split_cjk_phrase_on_fillers(token: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut seen = HashMap::<String, ()>::new();

    for ch in token.chars() {
        if is_cjk_filler_char(ch) || !is_cjk_or_digit(ch) {
            if current.chars().count() >= 2 {
                insert_unique_term(&mut seen, &mut segments, &current);
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }

    if current.chars().count() >= 2 {
        insert_unique_term(&mut seen, &mut segments, &current);
    }

    segments
}
