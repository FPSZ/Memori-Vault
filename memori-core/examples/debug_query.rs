use std::path::PathBuf;

use memori_core::{MEMORI_DB_PATH_ENV, MemoriEngine};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Natural language processing".to_string());
    let watch_root = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\AI\Tool\Memory\Memory_Test"));

    let db_path = std::env::var(MEMORI_DB_PATH_ENV).unwrap_or_else(|_| {
        dirs::data_dir()
            .unwrap()
            .join("Memori-Vault")
            .join(".memori.db")
            .to_string_lossy()
            .to_string()
    });
    unsafe {
        std::env::set_var(MEMORI_DB_PATH_ENV, db_path);
    }

    let engine = MemoriEngine::bootstrap(watch_root.clone())?;
    let loaded = engine.state().vector_store.load_from_db().await?;
    let scope_paths = vec![watch_root];
    let inspection = engine
        .retrieve_structured(&query, Some(scope_paths.as_slice()), Some(8))
        .await?;

    println!("query: {query}");
    println!("loaded_vectors: {loaded}");
    println!("status: {:?}", inspection.status);
    println!(
        "metrics: {}",
        serde_json::to_string_pretty(&inspection.metrics)?
    );
    println!("citations: {}", inspection.citations.len());
    println!("evidence: {}", inspection.evidence.len());
    for (index, evidence) in inspection.evidence.iter().take(8).enumerate() {
        let preview = evidence
            .content
            .chars()
            .take(260)
            .collect::<String>()
            .replace('\n', " ");
        println!(
            "#{} file={} chunk={} reason={} doc_rank={} chunk_rank={} preview={}",
            index + 1,
            evidence.file_path,
            evidence.chunk_index,
            evidence.reason,
            evidence.document_rank,
            evidence.chunk_rank,
            preview
        );
    }

    Ok(())
}
