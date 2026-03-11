use std::cmp::min;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use memori_parser::{ChunkBlockKind, DocumentChunk};
use memori_storage::SqliteStore;
use rusqlite::{Connection, params};
use serde::Serialize;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
struct Config {
    profiles: Vec<usize>,
    embedding_dim: usize,
    query_top_k: usize,
}

#[derive(Debug, Clone, Serialize)]
struct QueryMetrics {
    no_scope_ms: f64,
    dir_scope_ms: f64,
    file_scope_ms: f64,
    result_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct ContentionMetrics {
    reader_samples: usize,
    reader_p50_ms: f64,
    reader_p95_ms: f64,
    writer_ops: usize,
    writer_avg_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileMetrics {
    profile_label: String,
    document_count: usize,
    chunk_count: usize,
    embedding_dim: usize,
    cache_load_ms: f64,
    documents_fts: QueryMetrics,
    chunks_fts: QueryMetrics,
    dense_search: QueryMetrics,
    rebuild_ms: f64,
    documents_fts_after_rebuild_ms: f64,
    dense_search_after_rebuild_ms: f64,
    estimated_dense_cache_mb: f64,
    contention: Option<ContentionMetrics>,
}

#[derive(Debug, Clone, Serialize)]
struct Report {
    tool: &'static str,
    machine: MachineSnapshot,
    profiles: Vec<ProfileMetrics>,
}

#[derive(Debug, Clone, Serialize)]
struct MachineSnapshot {
    os: String,
    arch: String,
    cpus: usize,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), AnyError> {
    let config = parse_args()?;
    let machine = MachineSnapshot {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    };

    let mut results = Vec::new();
    for profile in &config.profiles {
        println!(
            "[phase2-diagnose] running profile docs={}, embedding_dim={}",
            profile, config.embedding_dim
        );
        results.push(run_profile(*profile, config.embedding_dim, config.query_top_k).await?);
    }

    let report = Report {
        tool: "memori-core/examples/phase2_diagnose.rs",
        machine,
        profiles: results,
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_profile(
    document_count: usize,
    embedding_dim: usize,
    top_k: usize,
) -> Result<ProfileMetrics, AnyError> {
    let temp_root = std::env::temp_dir().join(format!(
        "memori_phase2_diag_{}_{}",
        document_count,
        std::process::id()
    ));
    if temp_root.exists() {
        let _ = std::fs::remove_dir_all(&temp_root);
    }
    std::fs::create_dir_all(&temp_root)?;
    let db_path = temp_root.join("phase2_benchmark.db");
    let initializer = SqliteStore::new(&db_path)?;
    drop(initializer);

    let target_doc_index = min(document_count / 2, document_count.saturating_sub(1));
    let target_file = doc_file_path(&temp_root, target_doc_index);
    let target_dir = target_file.parent().unwrap().to_path_buf();
    ensure_scope_fixture(&target_file)?;

    let build_started = Instant::now();
    bulk_seed_documents(&db_path, &temp_root, document_count, embedding_dim)?;
    let _build_ms = build_started.elapsed().as_secs_f64() * 1000.0;

    let reopened = Arc::new(SqliteStore::new(&db_path)?);
    let cache_load_ms = measure_ms(|| async {
        let loaded = reopened.load_from_db().await?;
        assert_eq!(loaded, document_count);
        Ok::<(), AnyError>(())
    })
    .await?;

    let target_query = format!("doc-{:05}", target_doc_index);
    let target_topic = format!("topic-{:03}", target_doc_index % 113);
    let target_embedding = embedding_for_index(target_doc_index, embedding_dim);

    let documents_fts =
        benchmark_documents_fts(&reopened, &target_query, top_k, &target_dir, &target_file).await?;
    let chunks_fts =
        benchmark_chunks_fts(&reopened, &target_topic, top_k, &target_dir, &target_file).await?;
    let dense_search = benchmark_dense(
        &reopened,
        &target_embedding,
        top_k,
        &target_dir,
        &target_file,
    )
    .await?;

    let rebuild_ms = measure_ms(|| async {
        reopened.begin_full_rebuild("phase2_diagnose").await?;
        reopened.purge_all_index_data().await?;
        bulk_seed_documents(&db_path, &temp_root, document_count, embedding_dim)?;
        reopened.finish_full_rebuild().await?;
        let loaded = reopened.load_from_db().await?;
        assert_eq!(loaded, document_count);
        Ok::<(), AnyError>(())
    })
    .await?;

    let documents_fts_after_rebuild_ms = measure_ms(|| async {
        let _ = reopened
            .search_documents_fts(&target_query, top_k, &[])
            .await?;
        Ok::<(), AnyError>(())
    })
    .await?;
    let dense_search_after_rebuild_ms = measure_ms(|| async {
        let _ = reopened
            .search_similar_scoped(target_embedding.clone(), top_k, &[])
            .await?;
        Ok::<(), AnyError>(())
    })
    .await?;

    let contention = if document_count >= 10_000 {
        Some(
            benchmark_contention(
                Arc::clone(&reopened),
                temp_root.clone(),
                document_count,
                embedding_dim,
                top_k,
            )
            .await?,
        )
    } else {
        None
    };

    let estimated_dense_cache_mb =
        (document_count as f64 * embedding_dim as f64 * std::mem::size_of::<f32>() as f64)
            / (1024.0 * 1024.0);

    let metrics = ProfileMetrics {
        profile_label: format!("{} docs / {} chunks", document_count, document_count),
        document_count,
        chunk_count: document_count,
        embedding_dim,
        cache_load_ms,
        documents_fts,
        chunks_fts,
        dense_search,
        rebuild_ms,
        documents_fts_after_rebuild_ms,
        dense_search_after_rebuild_ms,
        estimated_dense_cache_mb,
        contention,
    };

    let _ = std::fs::remove_dir_all(&temp_root);
    Ok(metrics)
}

fn bulk_seed_documents(
    db_path: &Path,
    watch_root: &Path,
    document_count: usize,
    embedding_dim: usize,
) -> Result<(), AnyError> {
    let mut conn = Connection::open(db_path)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM chunks_fts", [])?;
    tx.execute("DELETE FROM documents_fts", [])?;
    tx.execute("DELETE FROM chunk_nodes", [])?;
    tx.execute("DELETE FROM edges", [])?;
    tx.execute("DELETE FROM nodes", [])?;
    tx.execute("DELETE FROM graph_task_queue", [])?;
    tx.execute("DELETE FROM chunks", [])?;
    tx.execute("DELETE FROM documents", [])?;
    tx.execute("DELETE FROM file_index_state", [])?;
    tx.execute("DELETE FROM file_catalog", [])?;

    {
        let mut catalog_stmt = tx.prepare(
            "INSERT INTO file_catalog(
                file_path, relative_path, file_name, file_ext, file_size, mtime_secs, discovered_at, removed_at
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
        )?;
        let mut document_stmt = tx.prepare(
            "INSERT INTO documents(
                file_path, relative_path, file_name, file_ext, last_modified, indexed_at,
                chunk_count, content_char_count, heading_catalog_text, document_search_text
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        let mut chunk_stmt = tx.prepare(
            "INSERT INTO chunks(
                doc_id, chunk_index, content, heading_path_json, block_kind, embedding_blob, char_len
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut chunk_fts_stmt = tx.prepare(
            "INSERT INTO chunks_fts(
                content, heading_text, file_name, relative_path, chunk_id, doc_id, file_path
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let mut document_fts_stmt = tx.prepare(
            "INSERT INTO documents_fts(
                search_text, file_name, relative_path, heading_catalog_text, doc_id, file_path
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        let mut index_state_stmt = tx.prepare(
            "INSERT INTO file_index_state(
                file_path, file_size, mtime_secs, content_hash, indexed_at,
                index_status, last_error, parser_format_version, index_format_version
             ) VALUES(?1, ?2, ?3, ?4, ?5, 'ready', NULL, 2, 2)",
        )?;

        for index in 0..document_count {
            if index > 0 && index % 10_000 == 0 {
                println!("[phase2-diagnose] bulk seeded {} docs", index);
            }

            let file_path = doc_file_path(watch_root, index);
            let file_path_text = normalize_storage_path(&file_path);
            let relative_path = normalize_relative_path(
                file_path
                    .strip_prefix(watch_root)
                    .unwrap_or(file_path.as_path())
                    .to_string_lossy()
                    .as_ref(),
            );
            let file_name = format!("doc-{index:05}.md");
            let heading_catalog_text = format!("Department {} / Topic {}", index % 16, index % 113);
            let content = format!(
                "doc-{index:05} topic-{topic:03} owner-{owner:03} release-{release:03} milestone summary unique-token-{index:05}",
                topic = index % 113,
                owner = index % 97,
                release = index % 53
            );
            let document_search_text =
                format!("{file_name}\n{relative_path}\n{heading_catalog_text}\n{content}");
            let embedding_blob = bincode::serialize(&embedding_for_index(index, embedding_dim))?;
            let heading_path = serde_json::to_string(&vec![
                format!("Department {}", index % 16),
                format!("Topic {}", index % 113),
            ])?;
            let char_len = content.chars().count() as i64;

            catalog_stmt.execute(params![
                file_path_text,
                relative_path,
                file_name,
                "md",
                content.len() as i64,
                index as i64,
                index as i64
            ])?;

            document_stmt.execute(params![
                file_path_text,
                relative_path,
                file_name,
                "md",
                index as i64,
                index as i64,
                1_i64,
                char_len,
                heading_catalog_text,
                document_search_text
            ])?;
            let doc_id = tx.last_insert_rowid();

            chunk_stmt.execute(params![
                doc_id,
                0_i64,
                content,
                heading_path,
                "paragraph",
                embedding_blob,
                char_len
            ])?;
            let chunk_id = tx.last_insert_rowid();

            chunk_fts_stmt.execute(params![
                content,
                heading_catalog_text,
                file_name,
                relative_path,
                chunk_id,
                doc_id,
                file_path_text
            ])?;

            document_fts_stmt.execute(params![
                document_search_text,
                file_name,
                relative_path,
                heading_catalog_text,
                doc_id,
                file_path_text
            ])?;

            index_state_stmt.execute(params![
                file_path_text,
                content.len() as i64,
                index as i64,
                format!("hash-{index:05}"),
                index as i64
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

async fn benchmark_documents_fts(
    store: &Arc<SqliteStore>,
    query: &str,
    top_k: usize,
    dir_scope: &Path,
    file_scope: &Path,
) -> Result<QueryMetrics, AnyError> {
    let (no_scope_ms, no_scope_count) = measure_query(|| async {
        store
            .search_documents_fts(query, top_k, &[])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (dir_scope_ms, _) = measure_query(|| async {
        store
            .search_documents_fts(query, top_k, &[dir_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (file_scope_ms, _) = measure_query(|| async {
        store
            .search_documents_fts(query, top_k, &[file_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    Ok(QueryMetrics {
        no_scope_ms,
        dir_scope_ms,
        file_scope_ms,
        result_count: no_scope_count,
    })
}

async fn benchmark_chunks_fts(
    store: &Arc<SqliteStore>,
    query: &str,
    top_k: usize,
    dir_scope: &Path,
    file_scope: &Path,
) -> Result<QueryMetrics, AnyError> {
    let (no_scope_ms, no_scope_count) = measure_query(|| async {
        store
            .search_chunks_fts(query, top_k, &[])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (dir_scope_ms, _) = measure_query(|| async {
        store
            .search_chunks_fts(query, top_k, &[dir_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (file_scope_ms, _) = measure_query(|| async {
        store
            .search_chunks_fts(query, top_k, &[file_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    Ok(QueryMetrics {
        no_scope_ms,
        dir_scope_ms,
        file_scope_ms,
        result_count: no_scope_count,
    })
}

async fn benchmark_dense(
    store: &Arc<SqliteStore>,
    query_embedding: &[f32],
    top_k: usize,
    dir_scope: &Path,
    file_scope: &Path,
) -> Result<QueryMetrics, AnyError> {
    let (no_scope_ms, no_scope_count) = measure_query(|| async {
        store
            .search_similar_scoped(query_embedding.to_vec(), top_k, &[])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (dir_scope_ms, _) = measure_query(|| async {
        store
            .search_similar_scoped(query_embedding.to_vec(), top_k, &[dir_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    let (file_scope_ms, _) = measure_query(|| async {
        store
            .search_similar_scoped(query_embedding.to_vec(), top_k, &[file_scope.to_path_buf()])
            .await
            .map(|rows| rows.len())
            .map_err(|err| -> AnyError { Box::new(err) })
    })
    .await?;
    Ok(QueryMetrics {
        no_scope_ms,
        dir_scope_ms,
        file_scope_ms,
        result_count: no_scope_count,
    })
}

async fn benchmark_contention(
    store: Arc<SqliteStore>,
    watch_root: PathBuf,
    document_count: usize,
    embedding_dim: usize,
    top_k: usize,
) -> Result<ContentionMetrics, AnyError> {
    let writer_store = Arc::clone(&store);
    let writer_root = watch_root.clone();
    let writer = tokio::spawn(async move {
        let mut samples = Vec::new();
        let writes = min(document_count, 200);
        for idx in 0..writes {
            let doc_index = idx * (document_count / writes.max(1)).max(1);
            let file_path = doc_file_path(&writer_root, doc_index);
            let chunk = DocumentChunk {
                file_path: file_path.clone(),
                content: format!(
                    "writer-refresh doc-{doc_index:05} topic-{:03}",
                    doc_index % 113
                ),
                chunk_index: 0,
                heading_path: vec![
                    format!("Department {}", doc_index % 16),
                    format!("Topic {}", doc_index % 113),
                ],
                block_kind: ChunkBlockKind::Paragraph,
            };
            let started = Instant::now();
            writer_store
                .replace_document_index(
                    &file_path,
                    Some(&writer_root),
                    doc_index as i64,
                    &format!("writer-hash-{doc_index:05}"),
                    vec![chunk],
                    vec![embedding_for_index(doc_index, embedding_dim)],
                )
                .await?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        Ok::<Vec<f64>, AnyError>(samples)
    });

    let mut readers = Vec::new();
    for reader_id in 0..4usize {
        let reader_store = Arc::clone(&store);
        let root = watch_root.clone();
        readers.push(tokio::spawn(async move {
            let mut latencies = Vec::new();
            for offset in 0..50usize {
                let doc_index = (reader_id * 997 + offset * 131) % document_count;
                let file_path = doc_file_path(&root, doc_index);
                let dir_scope = file_path.parent().unwrap().to_path_buf();
                let query = format!("doc-{doc_index:05}");
                let embedding = embedding_for_index(doc_index, embedding_dim);

                let started = Instant::now();
                let _ = reader_store
                    .search_documents_fts(&query, top_k, &[dir_scope.clone()])
                    .await?;
                let _ = reader_store
                    .search_similar_scoped(embedding, top_k, &[dir_scope])
                    .await?;
                latencies.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            Ok::<Vec<f64>, AnyError>(latencies)
        }));
    }

    let writer_samples = writer.await??;
    let mut reader_samples = Vec::new();
    for task in readers {
        reader_samples.extend(task.await??);
    }
    reader_samples.sort_by(|a, b| a.total_cmp(b));
    let writer_avg_ms = writer_samples.iter().sum::<f64>() / writer_samples.len().max(1) as f64;

    Ok(ContentionMetrics {
        reader_samples: reader_samples.len(),
        reader_p50_ms: percentile_ms(&reader_samples, 0.50),
        reader_p95_ms: percentile_ms(&reader_samples, 0.95),
        writer_ops: writer_samples.len(),
        writer_avg_ms,
    })
}

fn percentile_ms(samples: &[f64], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let last = samples.len().saturating_sub(1);
    let index = ((last as f64) * percentile).round() as usize;
    samples[index.min(last)]
}

async fn measure_ms<F, Fut, T>(operation: F) -> Result<f64, AnyError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, AnyError>>,
{
    let started = Instant::now();
    operation().await?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

async fn measure_query<F, Fut>(operation: F) -> Result<(f64, usize), AnyError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<usize, AnyError>>,
{
    let started = Instant::now();
    let count = operation().await?;
    Ok((started.elapsed().as_secs_f64() * 1000.0, count))
}

fn ensure_scope_fixture(target_file: &Path) -> Result<(), AnyError> {
    if let Some(parent) = target_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !target_file.exists() {
        std::fs::write(target_file, [])?;
    }
    Ok(())
}

fn doc_file_path(root: &Path, index: usize) -> PathBuf {
    root.join(format!(
        "department-{}/topic-{}/doc-{index:05}.md",
        index % 16,
        index % 113
    ))
}

fn embedding_for_index(index: usize, dim: usize) -> Vec<f32> {
    let mut state = index as u64 + 0x9E37_79B9;
    let mut values = Vec::with_capacity(dim);
    let mut norm = 0.0_f32;

    for offset in 0..dim {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407 + offset as u64);
        let raw = ((state >> 32) as u32 % 10_000) as f32 / 5_000.0 - 1.0;
        values.push(raw);
        norm += raw * raw;
    }

    let norm = norm.sqrt();
    if norm > 0.0 {
        for value in &mut values {
            *value /= norm;
        }
    }

    values
}

fn normalize_storage_path(path: &Path) -> String {
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

fn normalize_relative_path(text: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        text.replace('\\', "/")
    }

    #[cfg(not(target_os = "windows"))]
    {
        text.to_string()
    }
}

fn parse_args() -> Result<Config, AnyError> {
    let mut profiles = vec![1_000, 10_000, 50_000];
    let mut embedding_dim = 768usize;
    let mut query_top_k = 10usize;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profiles" => {
                let raw = args.next().ok_or("missing value for --profiles")?;
                profiles = raw
                    .split(',')
                    .map(|value| value.trim().parse::<usize>())
                    .collect::<Result<Vec<_>, _>>()?;
            }
            "--embedding-dim" => {
                embedding_dim = args
                    .next()
                    .ok_or("missing value for --embedding-dim")?
                    .parse::<usize>()?;
            }
            "--top-k" => {
                query_top_k = args
                    .next()
                    .ok_or("missing value for --top-k")?
                    .parse::<usize>()?;
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    Ok(Config {
        profiles,
        embedding_dim,
        query_top_k,
    })
}
