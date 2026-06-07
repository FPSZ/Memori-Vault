//! Graph-build speed benchmark: measures knowledge-graph extraction time vs
//! document character count (the "时间与文字数的比" metric).
//!
//! For each input file: parse → chunk → call `extract_entities` per chunk
//! (one graph LLM call each), and report total chars, chunk count, total ms,
//! ms/char and ms/chunk. Requires the graph LLM endpoint to be running
//! (default http://localhost:18002, override via MEMORI_GRAPH_ENDPOINT).
//!
//! Run:
//!   MEMORI_GRAPH_ENDPOINT=http://127.0.0.1:18002 MEMORI_GRAPH_MODEL=qwen3-8b \
//!   cargo run -q -p memori-core --example graph_bench -- <file1> <file2> ...

use std::path::Path;
use std::time::Instant;

use memori_core::extract_entities;
use memori_parser::{extract_document_text, parse_and_chunk};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

fn read_text(path: &Path) -> Result<String, AnyError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        ext.as_str(),
        "docx" | "pdf" | "pptx" | "xlsx" | "doc" | "ppt" | "xls"
    ) {
        extract_document_text(path)
            .ok_or_else(|| format!("extract failed: {}", path.display()).into())
    } else {
        Ok(std::fs::read_to_string(path)?)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), AnyError> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: graph_bench <file> [<file> ...]");
        std::process::exit(2);
    }

    println!(
        "{:<46} {:>7} {:>7} {:>9} {:>9} {:>9}",
        "file", "chars", "chunks", "total_ms", "ms/char", "ms/chunk"
    );
    println!("{}", "-".repeat(92));

    for file in &files {
        let path = Path::new(file);
        let text = match read_text(path) {
            Ok(t) => t,
            Err(err) => {
                eprintln!("[skip] {file}: {err}");
                continue;
            }
        };
        let chars = text.chars().count();
        let chunks = parse_and_chunk(path, &text)?;
        if chunks.is_empty() {
            eprintln!("[skip] {file}: no chunks (likely image-only/empty)");
            continue;
        }

        let started = Instant::now();
        let mut ok_chunks = 0usize;
        for chunk in &chunks {
            match extract_entities(&chunk.content).await {
                Ok(_) => ok_chunks += 1,
                Err(err) => eprintln!("[warn] graph extract failed on a chunk of {file}: {err}"),
            }
        }
        let total_ms = started.elapsed().as_millis() as f64;
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or(file);
        let short = if name.chars().count() > 44 {
            name.chars().take(44).collect::<String>()
        } else {
            name.to_string()
        };
        println!(
            "{:<46} {:>7} {:>7} {:>9.0} {:>9.3} {:>9.0}",
            short,
            chars,
            ok_chunks,
            total_ms,
            if chars > 0 {
                total_ms / chars as f64
            } else {
                0.0
            },
            if ok_chunks > 0 {
                total_ms / ok_chunks as f64
            } else {
                0.0
            },
        );
    }

    Ok(())
}
