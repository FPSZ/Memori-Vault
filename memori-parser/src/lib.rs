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
        "pptx" => extract_pptx_text(path),
        "xlsx" => extract_xlsx_text(path),
        "doc" => extract_doc_text(path),
        "ppt" => extract_ppt_text(path),
        "xls" => extract_xls_text(path),
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
    let mut out = String::new();
    let mut in_text = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.name();
                if name.as_ref() == b"w:t" {
                    in_text = true;
                } else if name.as_ref() == b"w:tab" {
                    out.push('\t');
                } else if name.as_ref() == b"w:br" {
                    out.push('\n');
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) => {
                let name = e.name();
                if name.as_ref() == b"w:tab" {
                    out.push('\t');
                } else if name.as_ref() == b"w:br" {
                    out.push('\n');
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                if name.as_ref() == b"w:t" {
                    in_text = false;
                } else if name.as_ref() == b"w:p" {
                    push_newline_if_needed(&mut out, 2);
                } else if name.as_ref() == b"w:tc" {
                    out.push('\t');
                } else if name.as_ref() == b"w:tr" {
                    push_newline_if_needed(&mut out, 1);
                }
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_text && let Ok(t) = e.unescape() {
                    out.push_str(&t);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Some(clean_extracted_document_text(&out))
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
    Some(clean_extracted_document_text(&raw))
}

// ============================================================================
// Legacy OLE2 (Office 97-2003) binary formats: .xls / .doc / .ppt
// These are NOT ZIP+XML; they are Compound File Binary (CFBF/OLE2) containers.
// .xls is delegated to the mature pure-Rust `calamine` crate; .doc/.ppt are
// best-effort lossy text extraction (enough for retrieval, not high fidelity).
// ============================================================================

/// Extract text from a legacy .xls (BIFF) workbook via `calamine`.
fn extract_xls_text(path: &Path) -> Option<String> {
    use calamine::Reader;
    debug!(path = %path.display(), "[解析器] 提取 XLS 文本");
    let mut workbook = calamine::open_workbook_auto(path).ok()?;
    let mut out = String::new();
    let names = workbook.sheet_names().to_owned();
    for name in &names {
        let Ok(range) = workbook.worksheet_range(name) else {
            continue;
        };
        for row in range.rows() {
            let cells: Vec<String> = row
                .iter()
                .map(xls_cell_to_string)
                .filter(|s| !s.trim().is_empty())
                .collect();
            if !cells.is_empty() {
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
        }
    }
    let cleaned = clean_extracted_document_text(&out);
    (!cleaned.is_empty()).then_some(cleaned)
}

fn xls_cell_to_string(cell: &calamine::Data) -> String {
    use calamine::Data;
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                (*f as i64).to_string()
            } else {
                f.to_string()
            }
        }
        Data::Bool(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

/// Read a named stream from an OLE2 compound file (case-sensitive entry name).
fn read_ole_stream(comp: &mut cfb::CompoundFile<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let target = comp
        .walk()
        .find(|entry| entry.is_stream() && entry.name() == name)
        .map(|entry| entry.path().to_path_buf())?;
    let mut stream = comp.open_stream(&target).ok()?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut stream, &mut buf).ok()?;
    Some(buf)
}

/// Map a Windows-1252 byte to a char (ASCII pass-through; Latin-1 for 0xA0+).
fn cp1252_char(b: u8) -> char {
    match b {
        0x80 => '€',
        0x82 => '‚',
        0x83 => 'ƒ',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => 'ˆ',
        0x89 => '‰',
        0x8A => 'Š',
        0x8B => '‹',
        0x8C => 'Œ',
        0x8E => 'Ž',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '“',
        0x94 => '”',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x98 => '˜',
        0x99 => '™',
        0x9A => 'š',
        0x9B => '›',
        0x9C => 'œ',
        0x9E => 'ž',
        0x9F => 'Ÿ',
        other => other as char,
    }
}

/// Append one decoded character, normalizing Word/PowerPoint control marks
/// (paragraph/line/cell breaks → newline; drop other control codes).
fn push_doc_char(out: &mut String, c: char) {
    match c {
        '\t' | '\n' => out.push(c),
        '\r' | '\u{0B}' | '\u{07}' | '\u{0C}' => out.push('\n'),
        c if (c as u32) < 0x20 => {}
        c => out.push(c),
    }
}

/// Extract text from a legacy .doc (Word 97-2003) via FIB + piece table (CLX).
/// Best-effort: relies on the piece table; returns None if it cannot be parsed.
fn extract_doc_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 DOC 文本");
    let file = std::fs::File::open(path).ok()?;
    let mut comp = cfb::CompoundFile::open(file).ok()?;
    let wd = read_ole_stream(&mut comp, "WordDocument")?;
    if wd.len() < 0x200 {
        return None;
    }
    // FIB flags @0x0A: bit fWhichTblStm (0x0200) selects 1Table vs 0Table.
    let flags = u16::from_le_bytes([wd[0x0A], wd[0x0B]]);
    let primary = if flags & 0x0200 != 0 {
        "1Table"
    } else {
        "0Table"
    };
    let table = read_ole_stream(&mut comp, primary)
        .or_else(|| read_ole_stream(&mut comp, "0Table"))
        .or_else(|| read_ole_stream(&mut comp, "1Table"))?;
    // fcClx @0x01A2, lcbClx @0x01A6 (offsets into the table stream).
    let fc_clx = u32::from_le_bytes([wd[0x01A2], wd[0x01A3], wd[0x01A4], wd[0x01A5]]) as usize;
    let lcb_clx = u32::from_le_bytes([wd[0x01A6], wd[0x01A7], wd[0x01A8], wd[0x01A9]]) as usize;
    if lcb_clx == 0 || fc_clx + lcb_clx > table.len() {
        return None;
    }
    let text = decode_doc_pieces(&wd, &table[fc_clx..fc_clx + lcb_clx])?;
    let cleaned = clean_extracted_document_text(&text);
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Decode the WordDocument text using the CLX piece table.
fn decode_doc_pieces(wd: &[u8], clx: &[u8]) -> Option<String> {
    // Walk CLX entries: 0x01 = Prc (skip), 0x02 = Pcdt (the PlcPcd we want).
    let mut i = 0usize;
    let mut pcdt: Option<&[u8]> = None;
    while i < clx.len() {
        match clx[i] {
            0x01 => {
                if i + 3 > clx.len() {
                    return None;
                }
                let cb = u16::from_le_bytes([clx[i + 1], clx[i + 2]]) as usize;
                i += 3 + cb;
            }
            0x02 => {
                if i + 5 > clx.len() {
                    return None;
                }
                let lcb =
                    u32::from_le_bytes([clx[i + 1], clx[i + 2], clx[i + 3], clx[i + 4]]) as usize;
                let start = i + 5;
                let end = start.checked_add(lcb)?;
                if end > clx.len() {
                    return None;
                }
                pcdt = Some(&clx[start..end]);
                break;
            }
            _ => return None,
        }
    }
    let pcdt = pcdt?;
    // PlcPcd: (n+1) CP u32 values, then n PCD structs (8 bytes each) => 12n + 4.
    if pcdt.len() < 16 {
        return None;
    }
    let n = (pcdt.len() - 4) / 12;
    if n == 0 {
        return None;
    }
    let cps: Vec<u32> = (0..=n)
        .map(|k| {
            u32::from_le_bytes([
                pcdt[k * 4],
                pcdt[k * 4 + 1],
                pcdt[k * 4 + 2],
                pcdt[k * 4 + 3],
            ])
        })
        .collect();
    let pcd_base = 4 * (n + 1);
    let mut out = String::new();
    for k in 0..n {
        let off = pcd_base + k * 8;
        if off + 8 > pcdt.len() {
            break;
        }
        // PCD.fc is bytes 2..6; bit 0x40000000 => 8-bit compressed (CP1252).
        let fc_field =
            u32::from_le_bytes([pcdt[off + 2], pcdt[off + 3], pcdt[off + 4], pcdt[off + 5]]);
        let cp_count = cps[k + 1].saturating_sub(cps[k]) as usize;
        if fc_field & 0x4000_0000 != 0 {
            let start = (fc_field & 0x3FFF_FFFF) as usize / 2;
            for b in wd.iter().skip(start).take(cp_count) {
                push_doc_char(&mut out, cp1252_char(*b));
            }
        } else {
            let start = (fc_field & 0x3FFF_FFFF) as usize;
            for j in 0..cp_count {
                let p = start + j * 2;
                if p + 1 >= wd.len() {
                    break;
                }
                let unit = u16::from_le_bytes([wd[p], wd[p + 1]]);
                if !(0xD800..=0xDFFF).contains(&unit)
                    && let Some(c) = char::from_u32(unit as u32)
                {
                    push_doc_char(&mut out, c);
                }
            }
        }
    }
    Some(out)
}

/// Extract text from a legacy .ppt (PowerPoint 97-2003) by walking the record
/// tree in the "PowerPoint Document" stream and collecting text atoms.
fn extract_ppt_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 PPT 文本");
    let file = std::fs::File::open(path).ok()?;
    let mut comp = cfb::CompoundFile::open(file).ok()?;
    let stream = read_ole_stream(&mut comp, "PowerPoint Document")?;
    let mut out = String::new();
    collect_ppt_text(&stream, &mut out, 0);
    let cleaned = clean_extracted_document_text(&out);
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Recursively walk PowerPoint records; collect TextCharsAtom (0x0FA0, UTF-16LE)
/// and TextBytesAtom (0x0FA8, CP1252). Containers have recVer nibble == 0xF.
fn collect_ppt_text(data: &[u8], out: &mut String, depth: usize) {
    if depth > 32 {
        return; // guard against malformed/deeply nested records
    }
    let mut i = 0usize;
    while i + 8 <= data.len() {
        let ver_inst = u16::from_le_bytes([data[i], data[i + 1]]);
        let rec_type = u16::from_le_bytes([data[i + 2], data[i + 3]]);
        let rec_len =
            u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        let body_start = i + 8;
        let body_end = body_start.saturating_add(rec_len).min(data.len());
        let body = &data[body_start..body_end];
        if ver_inst & 0x000F == 0x000F {
            collect_ppt_text(body, out, depth + 1);
        } else if rec_type == 0x0FA0 {
            let mut j = 0;
            while j + 1 < body.len() {
                let unit = u16::from_le_bytes([body[j], body[j + 1]]);
                if !(0xD800..=0xDFFF).contains(&unit)
                    && let Some(c) = char::from_u32(unit as u32)
                {
                    push_doc_char(out, c);
                }
                j += 2;
            }
            out.push('\n');
        } else if rec_type == 0x0FA8 {
            for b in body {
                push_doc_char(out, cp1252_char(*b));
            }
            out.push('\n');
        }
        i = body_end;
    }
}

/// Extract text from a .pptx file (Open XML PowerPoint presentation).
/// Pptx is a ZIP archive; each slide is `ppt/slides/slideN.xml` whose visible
/// text lives in `<a:t>` runs (the DrawingML analogue of docx `<w:t>`).
fn extract_pptx_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 PPTX 文本");
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let mut slide_names: Vec<String> = archive
        .file_names()
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .map(|name| name.to_string())
        .collect();
    slide_names.sort_by_key(|name| openxml_part_order(name, "ppt/slides/slide"));

    let mut out = String::new();
    for name in &slide_names {
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        let mut buf = Vec::new();
        if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
            continue;
        }
        extract_pptx_slide(&buf, &mut out);
        push_newline_if_needed(&mut out, 2);
    }

    Some(clean_extracted_document_text(&out))
}

/// Parse a single pptx slide XML buffer, appending its `<a:t>` text to `out`.
fn extract_pptx_slide(xml: &[u8], out: &mut String) {
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                if e.name().as_ref() == b"a:t" {
                    in_text = true;
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = e.name();
                if name.as_ref() == b"a:t" {
                    in_text = false;
                } else if name.as_ref() == b"a:p" {
                    push_newline_if_needed(out, 1);
                }
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_text && let Ok(t) = e.unescape() {
                    out.push_str(&t);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

/// Extract text from a .xlsx file (Open XML spreadsheet).
/// Cell text is either an index into `xl/sharedStrings.xml` (`t="s"`), an inline
/// string (`t="inlineStr"`), or a literal value in `<v>`. Rows become lines and
/// cells within a row are tab-separated.
fn extract_xlsx_text(path: &Path) -> Option<String> {
    debug!(path = %path.display(), "[解析器] 提取 XLSX 文本");
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    let shared = read_xlsx_shared_strings(&mut archive);

    let mut sheet_names: Vec<String> = archive
        .file_names()
        .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
        .map(|name| name.to_string())
        .collect();
    sheet_names.sort_by_key(|name| openxml_part_order(name, "xl/worksheets/sheet"));

    let mut out = String::new();
    for (idx, name) in sheet_names.iter().enumerate() {
        if idx > 0 {
            push_newline_if_needed(&mut out, 2);
        }
        let Ok(mut entry) = archive.by_name(name) else {
            continue;
        };
        let mut buf = Vec::new();
        if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
            continue;
        }
        extract_xlsx_sheet(&buf, &shared, &mut out);
    }

    Some(clean_extracted_document_text(&out))
}

/// Read `xl/sharedStrings.xml` into an ordered table; each `<si>` concatenates
/// all of its `<t>` runs (handles rich-text runs split across `<r>` elements).
fn read_xlsx_shared_strings(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<String> {
    let mut table = Vec::new();
    let Ok(mut entry) = archive.by_name("xl/sharedStrings.xml") else {
        return table;
    };
    let mut buf = Vec::new();
    if std::io::Read::read_to_end(&mut entry, &mut buf).is_err() {
        return table;
    }

    let mut reader = quick_xml::Reader::from_reader(&buf[..]);
    reader.config_mut().trim_text(true);
    let mut xbuf = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut xbuf) {
            Ok(quick_xml::events::Event::Start(e)) => match e.name().as_ref() {
                b"si" => {
                    current.clear();
                    in_si = true;
                }
                b"t" => in_text = true,
                _ => {}
            },
            Ok(quick_xml::events::Event::End(e)) => match e.name().as_ref() {
                b"si" => {
                    table.push(current.clone());
                    in_si = false;
                }
                b"t" => in_text = false,
                _ => {}
            },
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_si
                    && in_text
                    && let Ok(t) = e.unescape()
                {
                    current.push_str(&t);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        xbuf.clear();
    }

    table
}

/// Parse a single worksheet XML buffer, resolving shared-string cells and
/// appending tab-separated rows to `out`.
fn extract_xlsx_sheet(xml: &[u8], shared: &[String], out: &mut String) {
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut cell_is_shared = false;
    let mut in_value = false;
    let mut in_inline_text = false;
    let mut first_in_row = true;
    let mut row_has_cell = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => match e.name().as_ref() {
                b"row" => {
                    first_in_row = true;
                    row_has_cell = false;
                }
                b"c" => cell_is_shared = xlsx_cell_is_shared(&e),
                b"v" => in_value = true,
                b"t" => in_inline_text = true,
                _ => {}
            },
            Ok(quick_xml::events::Event::End(e)) => match e.name().as_ref() {
                b"v" => in_value = false,
                b"t" => in_inline_text = false,
                b"row" => {
                    if row_has_cell {
                        push_newline_if_needed(out, 1);
                    }
                }
                _ => {}
            },
            Ok(quick_xml::events::Event::Text(e)) => {
                if let Ok(raw) = e.unescape() {
                    if in_value {
                        let value = if cell_is_shared {
                            raw.trim()
                                .parse::<usize>()
                                .ok()
                                .and_then(|i| shared.get(i))
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            raw.to_string()
                        };
                        push_xlsx_cell(out, &value, &mut first_in_row, &mut row_has_cell);
                    } else if in_inline_text {
                        push_xlsx_cell(out, &raw, &mut first_in_row, &mut row_has_cell);
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

fn xlsx_cell_is_shared(e: &quick_xml::events::BytesStart) -> bool {
    e.attributes()
        .flatten()
        .any(|attr| attr.key.as_ref() == b"t" && attr.value.as_ref() == b"s")
}

fn push_xlsx_cell(out: &mut String, value: &str, first_in_row: &mut bool, row_has_cell: &mut bool) {
    if value.is_empty() {
        return;
    }
    if !*first_in_row {
        out.push('\t');
    }
    out.push_str(value);
    *first_in_row = false;
    *row_has_cell = true;
}

/// Numeric ordering key for OpenXML part names like `ppt/slides/slide12.xml`
/// or `xl/worksheets/sheet3.xml`, so parts sort 1,2,...,10 instead of 1,10,2.
fn openxml_part_order(name: &str, prefix: &str) -> u32 {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(".xml"))
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(u32::MAX)
}

fn push_newline_if_needed(out: &mut String, target_count: usize) {
    let current = out.chars().rev().take_while(|ch| *ch == '\n').count();
    for _ in current..target_count {
        out.push('\n');
    }
}

fn clean_extracted_document_text(text: &str) -> String {
    let normalized = text
        .replace("\u{00AD}", "")
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut lines = Vec::new();
    let mut blank_count = 0usize;
    for line in normalized.lines() {
        let line = line
            .split('\t')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        let line = line.trim();
        if line.is_empty() {
            blank_count += 1;
            if blank_count <= 1 && !lines.is_empty() {
                lines.push(String::new());
            }
        } else {
            blank_count = 0;
            lines.push(line.to_string());
        }
    }
    lines.join("\n").trim().to_string()
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

    fn build_zip(path: &std::path::Path, parts: &[(&str, &str)]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = zip::write::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, content) in parts {
            writer.start_file(*name, options).expect("start file");
            std::io::Write::write_all(&mut writer, content.as_bytes()).expect("write entry");
        }
        writer.finish().expect("finish zip");
    }

    fn unique_temp(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("memori_parser_{nanos}_{name}"))
    }

    #[test]
    fn extract_pptx_reads_slide_text_in_order() {
        let slide1 = r#"<?xml version="1.0"?><p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree>
<p:sp><p:txBody><a:p><a:r><a:t>银杏-18 冻结窗口</a:t></a:r></a:p>
<a:p><a:r><a:t>每周三 19:40 至 21:10</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#;
        let slide2 = r#"<?xml version="1.0"?><p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree>
<p:sp><p:txBody><a:p><a:r><a:t>第二页 负责人苏澈</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#;
        let path = unique_temp("deck.pptx");
        build_zip(
            &path,
            &[
                ("ppt/slides/slide1.xml", slide1),
                ("ppt/slides/slide2.xml", slide2),
            ],
        );

        let text = super::extract_document_text(&path).expect("extract pptx");
        let _ = std::fs::remove_file(&path);

        assert!(text.contains("银杏-18 冻结窗口"), "got: {text}");
        assert!(text.contains("每周三 19:40 至 21:10"), "got: {text}");
        assert!(text.contains("第二页 负责人苏澈"), "got: {text}");
        let p1 = text.find("银杏-18").unwrap();
        let p2 = text.find("第二页").unwrap();
        assert!(p1 < p2, "slide order not preserved: {text}");
    }

    #[test]
    fn extract_xlsx_resolves_shared_and_inline_cells() {
        let shared =
            r#"<?xml version="1.0"?><sst><si><t>参数项</t></si><si><t>轮换窗口</t></si></sst>"#;
        let sheet = r#"<?xml version="1.0"?><worksheet><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
<row r="2"><c r="A2"><v>43</v></c><c r="B2" t="inlineStr"><is><t>每月第二个周四22:10</t></is></c></row>
</sheetData></worksheet>"#;
        let path = unique_temp("params.xlsx");
        build_zip(
            &path,
            &[
                ("xl/sharedStrings.xml", shared),
                ("xl/worksheets/sheet1.xml", sheet),
            ],
        );

        let text = super::extract_document_text(&path).expect("extract xlsx");
        let _ = std::fs::remove_file(&path);

        assert!(text.contains("轮换窗口"), "shared string missing: {text}");
        assert!(text.contains("43"), "literal cell missing: {text}");
        assert!(
            text.contains("每月第二个周四22:10"),
            "inline string missing: {text}"
        );
    }
}
