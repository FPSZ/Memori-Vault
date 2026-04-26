use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use thiserror::Error;
use tracing::{debug, info, warn};

/// 单个文本块的数据结构。
#[derive(Debug, Clone)]
pub struct DocumentChunk {
    /// 文件来源路径，便于后续溯源与引用。
    pub file_path: PathBuf,
    /// 文本块正文。
    pub content: String,
    /// 当前块在文件中的顺序编号（从 0 开始）。
    pub chunk_index: usize,
    /// 当前块所在的标题路径。
    pub heading_path: Vec<String>,
    /// 当前块的语义类型。
    pub block_kind: ChunkBlockKind,
}

/// 当前块的语义类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkBlockKind {
    Heading,
    Paragraph,
    List,
    CodeBlock,
    Table,
    Quote,
    Html,
    ThematicBreak,
    Mixed,
}

/// 解析模块占位结构体（保留给 AppState 使用）。
#[derive(Debug, Default, Clone)]
pub struct ParserStub;

/// 最大块大小（字符数）。
pub const MAX_CHUNK_SIZE: usize = 1000;

/// 相邻块重叠大小（字符数）。
pub const OVERLAP_SIZE: usize = 200;

/// Parser 输出语义契约版本。
/// 仅当 chunk 边界、结构元数据或顺序语义发生不兼容变化时递增。
pub const PARSER_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
struct SemanticUnit {
    content: String,
    heading_path: Vec<String>,
    block_kind: ChunkBlockKind,
}

#[derive(Debug, Clone)]
struct OpenBlock {
    kind: ChunkBlockKind,
    start: usize,
    heading_path: Vec<String>,
}

/// 解析模块错误定义。
#[derive(Debug, Error)]
pub enum ParserError {
    #[error("非法分块配置：MAX_CHUNK_SIZE({max}) 必须大于 OVERLAP_SIZE({overlap})")]
    InvalidChunkConfig { max: usize, overlap: usize },
}

/// 对外统一入口：
/// 1) 使用 Markdown AST 切分语义块；
/// 2) 标题、列表、代码块、表格保持边界完整；
/// 3) 长段落仅在必要时做安全回退切分，并保留 overlap。
pub fn parse_and_chunk(
    file_path: impl AsRef<Path>,
    raw_text: &str,
) -> Result<Vec<DocumentChunk>, ParserError> {
    let file_path = file_path.as_ref();
    if MAX_CHUNK_SIZE <= OVERLAP_SIZE {
        return Err(ParserError::InvalidChunkConfig {
            max: MAX_CHUNK_SIZE,
            overlap: OVERLAP_SIZE,
        });
    }

    let normalized = raw_text.replace("\r\n", "\n").replace('\r', "\n");
    let units = collect_semantic_units(&normalized);
    if units.is_empty() {
        debug!(path = %file_path.display(), "解析结果为空，无语义单元");
        return Ok(Vec::new());
    }

    let chunks = assemble_chunks(&units);
    info!(path = %file_path.display(), units = units.len(), chunks = chunks.len(), "[解析器] 文档解析完成");

    let path_buf = file_path.to_path_buf();
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(chunk_index, unit)| DocumentChunk {
            file_path: path_buf.clone(),
            content: unit.content,
            chunk_index,
            heading_path: unit.heading_path,
            block_kind: unit.block_kind,
        })
        .collect())
}

fn collect_semantic_units(markdown: &str) -> Vec<SemanticUnit> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES;
    let parser = Parser::new_ext(markdown, options).into_offset_iter();

    let mut units = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut heading_capture: Option<(HeadingLevel, usize, String)> = None;
    let mut open_blocks: Vec<OpenBlock> = Vec::new();
    let mut list_depth = 0usize;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading_capture = Some((level, range.start, String::new()));
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some((captured_level, start, text)) = heading_capture.take() {
                    let end = range.end;
                    let source = markdown[start..end].trim().to_string();
                    let heading_text = normalize_inline_text(&text);
                    truncate_heading_stack(&mut heading_stack, captured_level);
                    if !heading_text.is_empty() {
                        heading_stack.push(heading_text);
                    }
                    if !source.is_empty() {
                        units.push(SemanticUnit {
                            content: source,
                            heading_path: heading_stack.clone(),
                            block_kind: ChunkBlockKind::Heading,
                        });
                    }
                } else {
                    truncate_heading_stack(&mut heading_stack, level);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                open_blocks.push(OpenBlock {
                    kind: ChunkBlockKind::CodeBlock,
                    start: range.start,
                    heading_path: heading_stack.clone(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_open_block(
                    markdown,
                    range.end,
                    ChunkBlockKind::CodeBlock,
                    &mut open_blocks,
                    &mut units,
                );
            }
            Event::Start(Tag::Paragraph) => {
                if list_depth > 0 || has_open_container(&open_blocks) {
                    continue;
                }
                open_blocks.push(OpenBlock {
                    kind: ChunkBlockKind::Paragraph,
                    start: range.start,
                    heading_path: heading_stack.clone(),
                });
            }
            Event::End(TagEnd::Paragraph) => {
                flush_open_block(
                    markdown,
                    range.end,
                    ChunkBlockKind::Paragraph,
                    &mut open_blocks,
                    &mut units,
                );
            }
            Event::Start(Tag::List(_)) => {
                if list_depth == 0 {
                    open_blocks.push(OpenBlock {
                        kind: ChunkBlockKind::List,
                        start: range.start,
                        heading_path: heading_stack.clone(),
                    });
                }
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    flush_open_block(
                        markdown,
                        range.end,
                        ChunkBlockKind::List,
                        &mut open_blocks,
                        &mut units,
                    );
                }
            }
            Event::Start(Tag::Table(_)) => {
                open_blocks.push(OpenBlock {
                    kind: ChunkBlockKind::Table,
                    start: range.start,
                    heading_path: heading_stack.clone(),
                });
            }
            Event::End(TagEnd::Table) => {
                flush_open_block(
                    markdown,
                    range.end,
                    ChunkBlockKind::Table,
                    &mut open_blocks,
                    &mut units,
                );
            }
            Event::Start(Tag::BlockQuote(_)) => {
                open_blocks.push(OpenBlock {
                    kind: ChunkBlockKind::Quote,
                    start: range.start,
                    heading_path: heading_stack.clone(),
                });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_open_block(
                    markdown,
                    range.end,
                    ChunkBlockKind::Quote,
                    &mut open_blocks,
                    &mut units,
                );
            }
            Event::Start(Tag::HtmlBlock) => {
                open_blocks.push(OpenBlock {
                    kind: ChunkBlockKind::Html,
                    start: range.start,
                    heading_path: heading_stack.clone(),
                });
            }
            Event::End(TagEnd::HtmlBlock) => {
                flush_open_block(
                    markdown,
                    range.end,
                    ChunkBlockKind::Html,
                    &mut open_blocks,
                    &mut units,
                );
            }
            Event::Rule => {
                let content = markdown[range.start..range.end].trim().to_string();
                if !content.is_empty() {
                    units.push(SemanticUnit {
                        content,
                        heading_path: heading_stack.clone(),
                        block_kind: ChunkBlockKind::ThematicBreak,
                    });
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, heading_text)) = heading_capture.as_mut() {
                    heading_text.push_str(text.as_ref());
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, heading_text)) = heading_capture.as_mut() {
                    heading_text.push(' ');
                }
            }
            _ => {}
        }
    }

    units
}

fn truncate_heading_stack(stack: &mut Vec<String>, level: HeadingLevel) {
    let keep = heading_level_to_usize(level).saturating_sub(1);
    while stack.len() > keep {
        stack.pop();
    }
}

fn heading_level_to_usize(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn flush_open_block(
    markdown: &str,
    end: usize,
    expected: ChunkBlockKind,
    open_blocks: &mut Vec<OpenBlock>,
    units: &mut Vec<SemanticUnit>,
) {
    let Some(index) = open_blocks.iter().rposition(|block| block.kind == expected) else {
        return;
    };
    let block = open_blocks.remove(index);
    let content = markdown[block.start..end].trim().to_string();
    if content.is_empty() {
        return;
    }
    units.push(SemanticUnit {
        content,
        heading_path: block.heading_path,
        block_kind: block.kind,
    });
}

fn assemble_chunks(units: &[SemanticUnit]) -> Vec<SemanticUnit> {
    let expanded_units = expand_units(units);
    let mut chunks: Vec<SemanticUnit> = Vec::new();
    let mut current: Option<SemanticUnit> = None;

    for unit in expanded_units {
        if is_hard_boundary(unit.block_kind) {
            if let Some(open) = current.take() {
                chunks.push(open);
            }
            chunks.push(unit);
            continue;
        }

        match current.as_mut() {
            Some(open)
                if open.heading_path == unit.heading_path
                    && can_append(&open.content, &unit.content, MAX_CHUNK_SIZE) =>
            {
                append_block(&mut open.content, &unit.content);
                if open.block_kind != unit.block_kind {
                    open.block_kind = ChunkBlockKind::Mixed;
                }
            }
            Some(_) => {
                let previous = current.take().expect("current chunk present");
                chunks.push(previous);
                current = Some(start_text_chunk(chunks.last(), &unit));
            }
            None => {
                current = Some(start_text_chunk(chunks.last(), &unit));
            }
        }
    }

    if let Some(open) = current {
        chunks.push(open);
    }

    chunks
}

fn expand_units(units: &[SemanticUnit]) -> Vec<SemanticUnit> {
    let mut expanded = Vec::new();

    for unit in units {
        if unit.block_kind == ChunkBlockKind::Paragraph && char_len(&unit.content) > MAX_CHUNK_SIZE
        {
            expanded.extend(split_long_unit(unit));
        } else {
            expanded.push(unit.clone());
        }
    }

    expanded
}

fn split_long_unit(unit: &SemanticUnit) -> Vec<SemanticUnit> {
    split_long_paragraph(
        &unit.content,
        MAX_CHUNK_SIZE.saturating_sub(OVERLAP_SIZE / 2),
    )
    .into_iter()
    .map(|content| SemanticUnit {
        content,
        heading_path: unit.heading_path.clone(),
        block_kind: unit.block_kind,
    })
    .collect()
}

fn is_hard_boundary(kind: ChunkBlockKind) -> bool {
    matches!(
        kind,
        ChunkBlockKind::Heading
            | ChunkBlockKind::List
            | ChunkBlockKind::CodeBlock
            | ChunkBlockKind::Table
            | ChunkBlockKind::Quote
            | ChunkBlockKind::Html
            | ChunkBlockKind::ThematicBreak
    )
}

fn start_text_chunk(previous: Option<&SemanticUnit>, unit: &SemanticUnit) -> SemanticUnit {
    let overlap = previous
        .filter(|prev| prev.heading_path == unit.heading_path)
        .map(|prev| tail_overlap_text(&prev.content, OVERLAP_SIZE))
        .unwrap_or_default();

    let mut content = overlap;
    append_block(&mut content, &unit.content);

    SemanticUnit {
        content,
        heading_path: unit.heading_path.clone(),
        block_kind: unit.block_kind,
    }
}

fn has_open_container(open_blocks: &[OpenBlock]) -> bool {
    open_blocks
        .iter()
        .any(|block| block.kind != ChunkBlockKind::Paragraph)
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
            ' ' | '\t'
                | ','
                | '，'
                | '。'
                | '；'
                | ';'
                | '！'
                | '!'
                | '？'
                | '?'
                | '、'
                | '：'
                | ':'
        ) {
            best = Some(pos);
        }
    }

    match best {
        Some(pos) if pos >= max_len / 2 => pos,
        _ => max_len,
    }
}

fn can_append(current: &str, block: &str, max_len: usize) -> bool {
    let separator_len = if current.trim().is_empty() { 0 } else { 2 };
    char_len(current) + separator_len + char_len(block) <= max_len
}

fn append_block(current: &mut String, block: &str) {
    if !current.trim().is_empty() {
        current.push_str("\n\n");
    }
    current.push_str(block.trim());
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

/// 生成相邻块重叠文本时，尽量从行边界开始，避免把 Markdown 语法切断。
fn tail_overlap_text(text: &str, count: usize) -> String {
    let tail = tail_chars(text, count);
    if tail.is_empty() {
        return tail;
    }

    if let Some(idx) = tail.find("\n\n") {
        let candidate = tail[idx + 2..].trim_start();
        if !candidate.is_empty() {
            return candidate.to_string();
        }
    }

    if let Some(idx) = tail.find('\n') {
        let candidate = tail[idx + 1..].trim_start();
        if !candidate.is_empty() {
            return candidate.to_string();
        }
    }

    tail.trim_start().to_string()
}

fn normalize_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{ChunkBlockKind, OVERLAP_SIZE, parse_and_chunk};

    #[test]
    fn headings_become_chunk_boundaries_with_metadata() {
        let markdown = "# Alpha\n\nFirst paragraph.\n\n## Beta\n\nSecond paragraph.";
        let chunks = parse_and_chunk("notes/test.md", markdown).expect("parse markdown");

        assert!(chunks.len() >= 4);
        assert_eq!(chunks[0].block_kind, ChunkBlockKind::Heading);
        assert_eq!(chunks[0].heading_path, vec!["Alpha"]);
        assert_eq!(chunks[1].heading_path, vec!["Alpha"]);
        assert_eq!(chunks[2].heading_path, vec!["Alpha", "Beta"]);
        assert_eq!(chunks[3].heading_path, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn code_block_and_list_stay_whole() {
        let markdown = "# Section\n\n- item one\n- item two\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```";
        let chunks = parse_and_chunk("notes/test.md", markdown).expect("parse markdown");

        assert!(chunks.iter().any(|chunk| {
            chunk.block_kind == ChunkBlockKind::List
                && chunk.content.contains("- item one")
                && chunk.content.contains("- item two")
        }));
        assert!(chunks.iter().any(|chunk| {
            chunk.block_kind == ChunkBlockKind::CodeBlock
                && chunk.content.contains("fn main()")
                && chunk.content.contains("println!")
        }));
    }

    #[test]
    fn long_paragraph_splits_and_preserves_heading_path() {
        let paragraph = "这是一个非常长的段落。".repeat(160);
        let markdown = format!("# Weekly Report\n\n{paragraph}");
        let chunks = parse_and_chunk("notes/test.md", &markdown).expect("parse markdown");

        let long_chunks: Vec<_> = chunks
            .iter()
            .filter(|chunk| chunk.block_kind != ChunkBlockKind::Heading)
            .collect();
        assert!(long_chunks.len() >= 2);
        assert!(
            long_chunks
                .iter()
                .all(|chunk| chunk.heading_path == vec!["Weekly Report"])
        );
        assert!(long_chunks[1].content.chars().count() >= OVERLAP_SIZE / 2);
    }
}

// ============================================================================
// Binary Document Text Extraction (docx / pdf)
// ============================================================================

/// Extract plain text from a file based on its extension.
/// Returns `None` if the file is not a supported binary format or extraction fails.
pub fn extract_document_text(file_path: impl AsRef<Path>) -> Option<String> {
    let path = file_path.as_ref();
    let ext = path.extension().and_then(|s| s.to_str())?;
    info!(path = %path.display(), ext = %ext, "[解析器] 提取二进制文档文本");
    let result = match ext.to_ascii_lowercase().as_str() {
        "docx" => extract_docx_text(path),
        "pdf" => extract_pdf_text(path),
        _ => None,
    };
    if result.is_none() {
        warn!(path = %path.display(), "[解析器] 文档文本提取失败");
    }
    result
}

/// Extract text from a .docx file (Open XML Word document).
/// Docx is a ZIP archive containing word/document.xml with <w:t> text runs.
fn extract_docx_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 DOCX 文本");
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut xml_reader = archive.by_name("word/document.xml").ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut xml_reader, &mut buf).ok()?;

    let mut reader = quick_xml::Reader::from_reader(&buf[..]);
    reader.config_mut().trim_text(true);
    let mut texts = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Text(e)) => {
                if let Ok(t) = e.unescape() {
                    texts.push(t.into_owned());
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let raw = texts.join("");
    // Replace soft-hyphens and normalize whitespace
    let cleaned = raw
        .replace("\u{00AD}", "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Some(cleaned)
}

/// Extract text from a PDF file using lopdf.
fn extract_pdf_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 PDF 文本");
    let doc = lopdf::Document::load(path).ok()?;
    let pages = doc.get_pages();
    let mut texts = Vec::with_capacity(pages.len());
    for (page_num, _) in pages {
        match doc.extract_text(&[page_num]) {
            Ok(page_text) => {
                texts.push(page_text);
            }
            Err(_) => continue,
        }
    }
    let raw = texts.join("\n");
    let cleaned = raw.replace("\r\n", "\n").replace('\r', "\n");
    Some(cleaned)
}
