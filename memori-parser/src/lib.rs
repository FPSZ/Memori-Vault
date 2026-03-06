use std::path::{Path, PathBuf};

use thiserror::Error;

/// 单个文本块的数据结构。
#[derive(Debug, Clone)]
pub struct DocumentChunk {
    /// 文件来源路径，便于后续溯源与引用。
    pub file_path: PathBuf,
    /// 文本块正文。
    pub content: String,
    /// 当前块在文件中的顺序编号（从 0 开始）。
    pub chunk_index: usize,
}

/// 解析模块占位结构体（保留给 AppState 使用）。
#[derive(Debug, Default, Clone)]
pub struct ParserStub;

/// 最大块大小（字符数）。
pub const MAX_CHUNK_SIZE: usize = 1000;

/// 相邻块重叠大小（字符数）。
pub const OVERLAP_SIZE: usize = 200;

/// 解析模块错误定义。
#[derive(Debug, Error)]
pub enum ParserError {
    #[error("非法分块配置：MAX_CHUNK_SIZE({max}) 必须大于 OVERLAP_SIZE({overlap})")]
    InvalidChunkConfig { max: usize, overlap: usize },
}

/// 对外统一入口：
/// 1) 段落优先切分（按双换行）；
/// 2) 将较短段落合并到尽量接近 MAX_CHUNK_SIZE；
/// 3) 为相邻块追加 OVERLAP_SIZE 重叠文本，避免语义边界断裂。
pub fn parse_and_chunk(
    file_path: impl AsRef<Path>,
    raw_text: &str,
) -> Result<Vec<DocumentChunk>, ParserError> {
    if MAX_CHUNK_SIZE <= OVERLAP_SIZE {
        return Err(ParserError::InvalidChunkConfig {
            max: MAX_CHUNK_SIZE,
            overlap: OVERLAP_SIZE,
        });
    }

    let normalized = raw_text.replace("\r\n", "\n").replace('\r', "\n");
    let mut units = collect_semantic_units(&normalized);
    if units.is_empty() {
        return Ok(Vec::new());
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut cursor = 0usize;

    while cursor < units.len() {
        let overlap = if let Some(prev) = chunks.last() {
            tail_chars(prev, OVERLAP_SIZE)
        } else {
            String::new()
        };

        let mut current = overlap.clone();

        while cursor < units.len() {
            let para = &units[cursor];
            if can_append(&current, para, MAX_CHUNK_SIZE) {
                append_paragraph(&mut current, para);
                cursor += 1;
                continue;
            }

            // 当前 chunk 只有 overlap，且下一个段落放不下：截取前缀填满本块，
            // 剩余部分留给下一轮继续处理，避免死循环。
            if char_len(&current) == char_len(&overlap) {
                let separator_len = if current.is_empty() { 0 } else { 2 };
                let available = MAX_CHUNK_SIZE
                    .saturating_sub(char_len(&current))
                    .saturating_sub(separator_len);

                if available == 0 {
                    break;
                }

                let head = take_prefix_chars(para, available);
                append_paragraph(&mut current, &head);
                let tail = drop_prefix_chars(para, available);

                if tail.trim().is_empty() {
                    cursor += 1;
                } else {
                    units[cursor] = tail;
                }
            }

            break;
        }

        // 安全兜底：理论上不应触发。
        if current.trim().is_empty() {
            cursor += 1;
            continue;
        }

        chunks.push(current);
    }

    let file_path = file_path.as_ref().to_path_buf();
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(chunk_index, content)| DocumentChunk {
            file_path: file_path.clone(),
            content,
            chunk_index,
        })
        .collect())
}

fn collect_semantic_units(text: &str) -> Vec<String> {
    let mut units = Vec::new();

    for paragraph in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if char_len(paragraph) <= MAX_CHUNK_SIZE {
            units.push(paragraph.to_string());
            continue;
        }

        units.extend(split_long_paragraph(paragraph, MAX_CHUNK_SIZE));
    }

    units
}

/// 长段落兜底切分：优先在空白/标点处断开，避免粗暴按字节硬切。
fn split_long_paragraph(paragraph: &str, max_len: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = paragraph.trim().to_string();

    while char_len(&remaining) > max_len {
        let cut = find_best_cut(&remaining, max_len);
        let head = take_prefix_chars(&remaining, cut).trim().to_string();
        let tail = drop_prefix_chars(&remaining, cut).trim_start().to_string();

        if !head.is_empty() {
            result.push(head);
        }

        remaining = tail;
        if remaining.is_empty() {
            break;
        }
    }

    if !remaining.trim().is_empty() {
        result.push(remaining.trim().to_string());
    }

    result
}

fn find_best_cut(text: &str, max_len: usize) -> usize {
    let mut best = None;

    for (idx, ch) in text.chars().enumerate() {
        let pos = idx + 1;
        if pos > max_len {
            break;
        }

        if matches!(
            ch,
            ' ' | '\t' | ',' | '，' | '。' | '；' | ';' | '！' | '!' | '？' | '?' | '、'
        ) {
            best = Some(pos);
        }
    }

    match best {
        Some(pos) if pos >= max_len / 2 => pos,
        _ => max_len,
    }
}

fn can_append(current: &str, paragraph: &str, max_len: usize) -> bool {
    let separator_len = if current.is_empty() { 0 } else { 2 };
    char_len(current) + separator_len + char_len(paragraph) <= max_len
}

fn append_paragraph(current: &mut String, paragraph: &str) {
    if !current.is_empty() {
        current.push_str("\n\n");
    }
    current.push_str(paragraph);
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn take_prefix_chars(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

fn drop_prefix_chars(text: &str, count: usize) -> String {
    text.chars().skip(count).collect()
}

fn tail_chars(text: &str, count: usize) -> String {
    let len = char_len(text);
    if len <= count {
        return text.to_string();
    }
    text.chars().skip(len - count).collect()
}
