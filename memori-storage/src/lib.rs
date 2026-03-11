use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use memori_parser::{ChunkBlockKind, DocumentChunk, PARSER_FORMAT_VERSION};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::info;

pub const DB_SCHEMA_VERSION: u32 = 2;
pub const INDEX_FORMAT_VERSION: u32 = 4;

const METADATA_KEY_INDEX_FORMAT_VERSION: &str = "index_format_version";
const METADATA_KEY_PARSER_FORMAT_VERSION: &str = "parser_format_version";
const METADATA_KEY_REBUILD_STATE: &str = "rebuild_state";
const METADATA_KEY_REBUILD_REASON: &str = "rebuild_reason";
const METADATA_KEY_LAST_REBUILD_AT: &str = "last_rebuild_at";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildState {
    Ready,
    Required,
    Rebuilding,
}

impl RebuildState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Required => "required",
            Self::Rebuilding => "rebuilding",
        }
    }

    fn from_stored(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "required" => Self::Required,
            "rebuilding" => Self::Rebuilding,
            _ => Self::Ready,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    pub index_format_version: u32,
    pub parser_format_version: u32,
    pub rebuild_state: RebuildState,
    pub rebuild_reason: Option<String>,
    pub last_rebuild_at: Option<i64>,
}

/// 图谱节点。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub name: String,
    pub description: Option<String>,
}

/// 图谱边。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdge {
    pub id: String,
    pub source_node: String,
    pub target_node: String,
    pub relation: String,
}

/// 存储在内存中的单条向量记录。
#[derive(Debug, Clone)]
pub struct StoredVectorRecord {
    pub chunk: DocumentChunk,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndexState {
    pub file_path: String,
    pub file_size: i64,
    pub mtime_secs: i64,
    pub content_hash: String,
    pub indexed_at: i64,
    pub index_status: String,
    pub last_error: Option<String>,
    pub parser_format_version: u32,
    pub index_format_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub file_path: String,
    pub relative_path: String,
    pub parent_dir: String,
    pub file_name: String,
    pub file_ext: String,
    pub file_size: i64,
    pub mtime_secs: i64,
    pub discovered_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentRecord {
    pub id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_ext: String,
    pub last_modified: i64,
    pub indexed_at: i64,
    pub chunk_count: u32,
    pub content_char_count: u32,
    pub heading_catalog_text: String,
    pub document_search_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkRecord {
    pub id: i64,
    pub doc_id: i64,
    pub chunk_index: usize,
    pub content: String,
    pub heading_path: Vec<String>,
    pub block_kind: String,
    pub char_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FtsChunkMatch {
    pub chunk_id: i64,
    pub doc_id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub chunk_index: usize,
    pub score: f64,
    pub content: String,
    pub heading_path: Vec<String>,
    pub block_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FtsDocumentMatch {
    pub doc_id: i64,
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub score: f64,
    pub heading_catalog_text: String,
    pub document_search_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentSignalMatch {
    pub file_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub matched_fields: Vec<String>,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTaskRecord {
    pub task_id: i64,
    pub chunk_id: i64,
    pub content: String,
    pub content_hash: String,
    pub status: String,
    pub retry_count: i64,
}

#[derive(Debug, Clone)]
struct CachedVector {
    chunk_id: i64,
    doc_id: i64,
    file_path: PathBuf,
    embedding: Vec<f32>,
}

const INDEX_STATUS_READY: &str = "ready";
const INDEX_STATUS_PENDING: &str = "pending";
const INDEX_STATUS_FAILED: &str = "failed";
const DOCUMENT_SEARCH_PREVIEW_CHARS: usize = 2_000;
const DOCUMENT_SEARCH_SNIPPET_CHARS: usize = 180;
const DOCUMENT_SEARCH_MAX_SNIPPETS: usize = 8;
const DOCUMENT_SEARCH_SYMBOL_CHARS: usize = 6_000;

/// 向量存储统一抽象。
#[allow(async_fn_in_trait)]
pub trait VectorStore: Send + Sync {
    async fn insert_chunks(
        &self,
        chunks: Vec<DocumentChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), StorageError>;

    async fn search_similar(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
    ) -> Result<Vec<(DocumentChunk, f32)>, StorageError>;
}

/// 基于 RwLock 的内存向量存储实现。
#[derive(Debug, Default)]
pub struct InMemoryStore {
    records: RwLock<Vec<StoredVectorRecord>>,
}

mod document;
mod schema;
mod search;
mod store;
mod text;
mod vector;

pub(crate) use schema::*;
pub(crate) use text::*;

#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
    cache: RwLock<Vec<CachedVector>>,
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogEntry, INDEX_FORMAT_VERSION, RebuildState, SqliteStore, build_document_search_text,
        extract_fts_terms, extract_phrase_signal_terms, extract_signal_terms,
    };
    use crate::VectorStore;
    use memori_parser::DocumentChunk;
    use memori_parser::PARSER_FORMAT_VERSION;
    use rusqlite::Connection;

    fn unique_db_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn replace_document_index_round_trips_metadata_and_fts() {
        let db_path = unique_db_path("memori_vault_storage_roundtrip");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from("notes");
        let file_path = watch_root.join("project").join("weekly.md");
        let chunks = vec![
            DocumentChunk {
                file_path: file_path.clone(),
                content: "Alpha rollout checklist".to_string(),
                chunk_index: 0,
                heading_path: vec!["Project".to_string(), "Weekly".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: file_path.clone(),
                content: "```rust\nfn main() {}\n```".to_string(),
                chunk_index: 1,
                heading_path: vec!["Project".to_string(), "Implementation".to_string()],
                block_kind: memori_parser::ChunkBlockKind::CodeBlock,
            },
        ];

        store
            .replace_document_index(
                &file_path,
                Some(&watch_root),
                123,
                "content_hash_v1",
                chunks,
                vec![vec![1.0_f32, 0.0_f32], vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("replace document index");

        let document = store
            .get_document_by_file_path(&file_path)
            .await
            .expect("get document")
            .expect("document exists");
        assert_eq!(document.relative_path, "project/weekly.md");
        assert_eq!(document.file_name, "weekly.md");
        assert_eq!(document.chunk_count, 2);
        assert!(
            document
                .document_search_text
                .contains("Alpha rollout checklist")
        );
        assert!(document.heading_catalog_text.contains("Project / Weekly"));

        let chunk_records = store
            .get_chunks_by_doc_id(document.id)
            .await
            .expect("get chunk records");
        assert_eq!(chunk_records.len(), 2);
        assert_eq!(
            chunk_records[0].heading_path,
            vec!["Project".to_string(), "Weekly".to_string()]
        );
        assert_eq!(chunk_records[1].block_kind, "code_block");

        let lexical_chunks = store
            .search_chunks_fts("Alpha weekly", 5, &[])
            .await
            .expect("search chunks fts");
        assert!(!lexical_chunks.is_empty());
        assert!(lexical_chunks.iter().any(|item| {
            item.file_name == "weekly.md"
                && item.heading_path == vec!["Project".to_string(), "Weekly".to_string()]
        }));

        let lexical_docs = store
            .search_documents_fts("project weekly", 5, &[])
            .await
            .expect("search documents fts");
        assert_eq!(lexical_docs.len(), 1);
        assert_eq!(lexical_docs[0].relative_path, "project/weekly.md");

        let scoped_docs = store
            .search_documents_fts("weekly", 5, &[watch_root.join("project")])
            .await
            .expect("search scoped docs");
        assert_eq!(scoped_docs.len(), 1);
        let blocked_docs = store
            .search_documents_fts("weekly", 5, &[watch_root.join("other")])
            .await
            .expect("search blocked docs");
        assert!(blocked_docs.is_empty());

        let signal_docs = store
            .search_documents_signal("weekly.md project", 5, &[])
            .await
            .expect("search deterministic docs");
        assert_eq!(signal_docs.len(), 1);
        assert!(
            signal_docs[0]
                .matched_fields
                .iter()
                .any(|field| field == "file_name")
        );
        assert!(
            signal_docs[0]
                .matched_fields
                .iter()
                .any(|field| field == "relative_path")
        );

        let semantic = store
            .search_similar_scoped(vec![1.0_f32, 0.0_f32], 1, &[])
            .await
            .expect("search similar scoped");
        assert_eq!(semantic.len(), 1);
        assert_eq!(
            semantic[0].0.heading_path,
            vec!["Project".to_string(), "Weekly".to_string()]
        );
        assert_eq!(
            semantic[0].0.block_kind,
            memori_parser::ChunkBlockKind::Paragraph
        );
        assert_eq!(
            store
                .embedding_dimension()
                .await
                .expect("embedding dimension"),
            Some(2)
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn document_search_text_samples_late_chunks_across_document() {
        let catalog_entry = CatalogEntry {
            file_path: "notes/project/weekly.md".to_string(),
            relative_path: "project/weekly.md".to_string(),
            parent_dir: "project".to_string(),
            file_name: "weekly.md".to_string(),
            file_ext: ".md".to_string(),
            file_size: 0,
            mtime_secs: 0,
            discovered_at: 0,
            removed_at: None,
        };
        let chunks = vec![
            DocumentChunk {
                file_path: std::path::PathBuf::from("notes/project/weekly.md"),
                content: "opening section ".repeat(40),
                chunk_index: 0,
                heading_path: vec!["Intro".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: std::path::PathBuf::from("notes/project/weekly.md"),
                content: "late unique signal: settings Advanced tab".to_string(),
                chunk_index: 1,
                heading_path: vec!["Settings".to_string()],
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
        ];

        let document_search_text =
            build_document_search_text(&catalog_entry, "Intro\nSettings", &chunks);

        assert!(document_search_text.contains("weekly.md"));
        assert!(document_search_text.contains("late unique signal: settings Advanced tab"));
    }

    #[test]
    fn code_document_search_text_includes_symbols_and_literals() {
        let catalog_entry = CatalogEntry {
            file_path: "memori-core/src/lib.rs".to_string(),
            relative_path: "memori-core/src/lib.rs".to_string(),
            parent_dir: "memori-core/src".to_string(),
            file_name: "lib.rs".to_string(),
            file_ext: ".rs".to_string(),
            file_size: 0,
            mtime_secs: 0,
            discovered_at: 0,
            removed_at: None,
        };
        let chunks = vec![DocumentChunk {
            file_path: std::path::PathBuf::from("memori-core/src/lib.rs"),
            content: r#"
                pub struct RetrievalMetrics {
                    pub query_analysis_ms: u64,
                }

                pub async fn ask_vault_structured() {}
                const ASK_ROUTE: &str = "POST /api/ask";
            "#
            .to_string(),
            chunk_index: 0,
            heading_path: Vec::new(),
            block_kind: memori_parser::ChunkBlockKind::CodeBlock,
        }];

        let document_search_text = build_document_search_text(&catalog_entry, "", &chunks);

        assert!(document_search_text.contains("query_analysis_ms"));
        assert!(document_search_text.contains("ask_vault_structured"));
        assert!(document_search_text.contains("POST /api/ask"));
    }

    #[tokio::test]
    async fn search_documents_signal_matches_code_symbols_before_broad_text() {
        let db_path = unique_db_path("memori_vault_storage_code_signal");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from(".");
        let code_file = std::path::PathBuf::from("memori-core/src/lib.rs");
        let readme_file = std::path::PathBuf::from("README.md");

        store
            .replace_document_index(
                &code_file,
                Some(&watch_root),
                123,
                "code_hash",
                vec![DocumentChunk {
                    file_path: code_file.clone(),
                    content: r#"
                        pub struct RetrievalMetrics {
                            pub query_analysis_ms: u64,
                        }

                        pub async fn ask_vault_structured() {}
                        const ASK_ROUTE: &str = "POST /api/ask";
                    "#
                    .to_string(),
                    chunk_index: 0,
                    heading_path: Vec::new(),
                    block_kind: memori_parser::ChunkBlockKind::CodeBlock,
                }],
                vec![vec![1.0_f32, 0.0_f32]],
            )
            .await
            .expect("index code file");

        store
            .replace_document_index(
                &readme_file,
                Some(&watch_root),
                123,
                "readme_hash",
                vec![DocumentChunk {
                    file_path: readme_file.clone(),
                    content: "This README mentions ask and api concepts in broad prose."
                        .to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Overview".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("index readme file");

        let symbol_hits = store
            .search_documents_signal("ask_vault_structured POST /api/ask", 5, &[])
            .await
            .expect("search symbol docs");

        assert!(symbol_hits.first().is_some_and(|item| {
            item.file_path
                .replace('\\', "/")
                .ends_with("memori-core/src/lib.rs")
        }));
        assert!(
            symbol_hits[0]
                .matched_fields
                .iter()
                .any(|field| field == "exact_symbol")
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn search_documents_phrase_signal_prefers_docs_phrase_matches() {
        let db_path = unique_db_path("memori_vault_storage_phrase_signal");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let watch_root = std::path::PathBuf::from(".");
        let tutorial_file = std::path::PathBuf::from("docs/TUTORIAL.md");
        let readme_file = std::path::PathBuf::from("README.md");

        store
            .replace_document_index(
                &tutorial_file,
                Some(&watch_root),
                123,
                "tutorial_hash",
                vec![DocumentChunk {
                    file_path: tutorial_file.clone(),
                    content: "How to start server mode:\n\ncargo run -p memori-server".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Server".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![1.0_f32, 0.0_f32]],
            )
            .await
            .expect("index tutorial file");

        store
            .replace_document_index(
                &readme_file,
                Some(&watch_root),
                123,
                "readme_hash",
                vec![DocumentChunk {
                    file_path: readme_file.clone(),
                    content: "Server runtime overview and product summary.".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["Overview".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.0_f32, 1.0_f32]],
            )
            .await
            .expect("index readme file");

        let phrase_hits = store
            .search_documents_phrase_signal("How do you start server mode?", 5, &[])
            .await
            .expect("search docs phrase signal");

        assert!(
            phrase_hits.iter().any(|item| {
                item.file_path
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .ends_with("docs/tutorial.md")
                    && item
                        .matched_fields
                        .iter()
                        .any(|field| field == "docs_phrase")
            }),
            "phrase_hits={phrase_hits:?}"
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn fts_terms_keep_cjk_meaningful_phrase_and_identifiers() {
        let terms = extract_fts_terms("长跳转公式是什么 POST /api/ask week8_report.md");
        assert!(terms.iter().any(|term| term == "长跳转公式"));
        assert!(!terms.iter().any(|term| term == "是什么"));
        assert!(terms.iter().any(|term| term == "post"));
        assert!(terms.iter().any(|term| term == "api"));
        assert!(terms.iter().any(|term| term == "ask"));
        assert!(terms.iter().any(|term| term == "week8_report.md"));
        assert!(terms.iter().any(|term| term == "week8_report"));
    }

    #[test]
    fn signal_terms_keep_mixed_cjk_and_digits() {
        let terms = extract_signal_terms("周报8 week8_report.md");
        assert!(terms.iter().any(|term| term == "周报8"));
        assert!(terms.iter().any(|term| term == "周报"));
        assert!(terms.iter().any(|term| term == "week8_report.md"));
        assert!(terms.iter().any(|term| term == "week8_report"));
    }

    #[test]
    fn phrase_signal_terms_keep_docs_and_api_phrases() {
        let terms = extract_phrase_signal_terms("What does POST /api/auth/oidc/login return?");
        assert!(terms.iter().any(|term| term == "post /api/auth/oidc/login"));

        let docs_terms = extract_phrase_signal_terms("How do you start server mode?");
        assert!(docs_terms.iter().any(|term| term == "start server mode"));
    }

    #[tokio::test]
    async fn purge_file_path_removes_document_chunks_and_index_state() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_storage_purge_{}.db",
            std::process::id()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let file_path = std::path::PathBuf::from("notes/test.md");
        let chunks = vec![
            DocumentChunk {
                file_path: file_path.clone(),
                content: "hello".to_string(),
                chunk_index: 0,
                heading_path: Vec::new(),
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
            DocumentChunk {
                file_path: file_path.clone(),
                content: "world".to_string(),
                chunk_index: 1,
                heading_path: Vec::new(),
                block_kind: memori_parser::ChunkBlockKind::Paragraph,
            },
        ];
        let embeddings = vec![vec![0.1_f32, 0.2_f32], vec![0.3_f32, 0.4_f32]];

        store
            .insert_chunks(chunks, embeddings)
            .await
            .expect("insert chunks");
        store
            .upsert_file_index_state(&file_path, 10, 20, "hash")
            .await
            .expect("upsert file index state");

        let purged = store
            .purge_file_path(&file_path)
            .await
            .expect("purge file path");
        assert!(purged);

        assert!(
            store
                .resolve_chunk_id(&file_path, 0)
                .await
                .expect("resolve chunk id")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index state")
                .is_none()
        );

        let purged_again = store
            .purge_file_path(&file_path)
            .await
            .expect("purge missing file path");
        assert!(!purged_again);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn purge_directory_path_removes_nested_documents_and_index_state() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_storage_purge_dir_{}.db",
            std::process::id()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let nested_a = std::path::PathBuf::from("notes/project/a.md");
        let nested_b = std::path::PathBuf::from("notes/project/sub/b.txt");
        let outside = std::path::PathBuf::from("notes/other/c.md");

        for file_path in [&nested_a, &nested_b, &outside] {
            store
                .insert_chunks(
                    vec![DocumentChunk {
                        file_path: file_path.clone(),
                        content: format!("content for {}", file_path.display()),
                        chunk_index: 0,
                        heading_path: Vec::new(),
                        block_kind: memori_parser::ChunkBlockKind::Paragraph,
                    }],
                    vec![vec![0.1_f32, 0.2_f32]],
                )
                .await
                .expect("insert chunks");
            store
                .upsert_file_index_state(file_path, 10, 20, "hash")
                .await
                .expect("upsert file index state");
        }

        let purged = store
            .purge_directory_path(&std::path::PathBuf::from("notes/project"))
            .await
            .expect("purge directory path");
        assert!(purged);

        assert!(
            store
                .resolve_chunk_id(&nested_a, 0)
                .await
                .expect("resolve nested a")
                .is_none()
        );
        assert!(
            store
                .resolve_chunk_id(&nested_b, 0)
                .await
                .expect("resolve nested b")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&nested_a)
                .await
                .expect("state nested a")
                .is_none()
        );
        assert!(
            store
                .get_file_index_state(&nested_b)
                .await
                .expect("state nested b")
                .is_none()
        );
        assert!(
            store
                .resolve_chunk_id(&outside, 0)
                .await
                .expect("resolve outside")
                .is_some()
        );

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn new_store_initializes_ready_index_metadata() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_meta_ready_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let metadata = store
            .read_index_metadata()
            .await
            .expect("read index metadata");

        assert_eq!(metadata.rebuild_state, RebuildState::Ready);
        assert_eq!(metadata.index_format_version, INDEX_FORMAT_VERSION);
        assert_eq!(metadata.parser_format_version, PARSER_FORMAT_VERSION);
        assert!(metadata.rebuild_reason.is_none());

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn existing_index_without_metadata_is_marked_for_rebuild() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_meta_required_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let conn = Connection::open(&db_path).expect("open raw sqlite db");
        conn.execute_batch(
            "CREATE TABLE documents (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_path TEXT NOT NULL UNIQUE,
                 last_modified INTEGER NOT NULL
             );
             CREATE TABLE chunks (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 doc_id INTEGER NOT NULL,
                 chunk_index INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 embedding_blob BLOB NOT NULL
             );
             CREATE TABLE file_index_state (
                 file_path TEXT PRIMARY KEY,
                 file_size INTEGER NOT NULL,
                 mtime_secs INTEGER NOT NULL,
                 content_hash TEXT NOT NULL,
                 indexed_at INTEGER NOT NULL
             );
             CREATE TABLE graph_task_queue (
                 task_id INTEGER PRIMARY KEY AUTOINCREMENT,
                 chunk_id INTEGER NOT NULL,
                 content TEXT NOT NULL,
                 content_hash TEXT NOT NULL,
                 status TEXT NOT NULL,
                 retry_count INTEGER NOT NULL DEFAULT 0,
                 updated_at INTEGER NOT NULL
             );",
        )
        .expect("seed legacy schema");
        conn.execute(
            "INSERT INTO documents(file_path, last_modified) VALUES(?1, ?2)",
            rusqlite::params!["notes/legacy.md", 1_i64],
        )
        .expect("insert legacy doc");
        drop(conn);

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let metadata = store
            .read_index_metadata()
            .await
            .expect("read index metadata");

        assert_eq!(metadata.rebuild_state, RebuildState::Required);
        assert_eq!(
            metadata.rebuild_reason.as_deref(),
            Some("index_metadata_missing")
        );
        assert_eq!(metadata.index_format_version, 0);
        assert_eq!(metadata.parser_format_version, 0);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn existing_legacy_file_catalog_is_upgraded_with_parent_dir_and_removed_at() {
        let db_path = unique_db_path("memori_vault_legacy_file_catalog_columns");
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let conn = Connection::open(&db_path).expect("open raw sqlite db");
        conn.execute_batch(
            "CREATE TABLE file_catalog (
                 file_path TEXT PRIMARY KEY,
                 relative_path TEXT NOT NULL,
                 file_name TEXT NOT NULL,
                 file_ext TEXT NOT NULL,
                 file_size INTEGER NOT NULL,
                 mtime_secs INTEGER NOT NULL,
                 discovered_at INTEGER NOT NULL
             );",
        )
        .expect("seed legacy file_catalog schema");
        drop(conn);

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let conn = store.lock_conn().expect("lock sqlite conn");
        let mut stmt = conn
            .prepare("PRAGMA table_info(file_catalog)")
            .expect("prepare pragma");
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query pragma")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect columns");

        assert!(columns.iter().any(|column| column == "parent_dir"));
        assert!(columns.iter().any(|column| column == "removed_at"));

        drop(stmt);
        drop(conn);
        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }

    #[tokio::test]
    async fn purge_all_index_data_clears_relational_rows_and_cache() {
        let db_path = std::env::temp_dir().join(format!(
            "memori_vault_store_purge_all_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("duration since epoch")
                .as_nanos()
        ));
        if db_path.exists() {
            let _ = std::fs::remove_file(&db_path);
        }

        let store = SqliteStore::new(&db_path).expect("create sqlite store");
        let file_path = std::path::PathBuf::from("notes/purge-all.md");
        store
            .insert_chunks(
                vec![DocumentChunk {
                    file_path: file_path.clone(),
                    content: "hello".to_string(),
                    chunk_index: 0,
                    heading_path: vec!["H1".to_string()],
                    block_kind: memori_parser::ChunkBlockKind::Paragraph,
                }],
                vec![vec![0.1_f32, 0.2_f32]],
            )
            .await
            .expect("insert chunks");
        store
            .upsert_file_index_state(&file_path, 11, 22, "hash")
            .await
            .expect("upsert file index");
        let chunk_id = store
            .resolve_chunk_id(&file_path, 0)
            .await
            .expect("resolve chunk id")
            .expect("chunk id exists");
        store
            .enqueue_graph_task(chunk_id, "hash", "hello")
            .await
            .expect("enqueue graph task");

        store
            .purge_all_index_data()
            .await
            .expect("purge all index data");

        assert_eq!(store.count_documents().await.expect("count documents"), 0);
        assert_eq!(store.count_chunks().await.expect("count chunks"), 0);
        assert_eq!(
            store
                .count_graph_backlog()
                .await
                .expect("count graph backlog"),
            0
        );
        assert!(
            store
                .get_file_index_state(&file_path)
                .await
                .expect("get file index state")
                .is_none()
        );
        assert_eq!(store.load_from_db().await.expect("load from db"), 0);

        drop(store);
        let _ = std::fs::remove_file(&db_path);
    }
}

/// 存储层错误定义。
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("向量数量与文本块数量不一致: chunks={chunks}, embeddings={embeddings}")]
    LengthMismatch { chunks: usize, embeddings: usize },

    #[error("插入批次为空，无法写入数据库")]
    EmptyChunks,

    #[error("同一批次出现多个文件路径，拒绝写入")]
    MixedFilePathInBatch,

    #[error("chunk_id 不存在: {chunk_id}")]
    ChunkNotFound { chunk_id: i64 },

    #[error("chunk_index 超出可存储范围: {chunk_index}")]
    ChunkIndexOverflow { chunk_index: usize },

    #[error("数据库中的 chunk_index 非法: {raw}")]
    InvalidChunkIndex { raw: i64 },

    #[error("SQLite 连接锁已损坏: {0}")]
    LockPoisoned(&'static str),

    #[error("数据库当前被占用，请稍后重试: {0}")]
    DatabaseLocked(#[source] rusqlite::Error),

    #[error("SQLite 操作失败: {0}")]
    Sqlite(#[source] rusqlite::Error),

    #[error("JSON 序列化/反序列化失败: {0}")]
    SerdeJson(#[source] serde_json::Error),

    #[error("Embedding 序列化失败: {0}")]
    SerializeEmbedding(#[source] bincode::Error),

    #[error("Embedding 反序列化失败: {0}")]
    DeserializeEmbedding(#[source] bincode::Error),

    #[error("系统时间异常: {0}")]
    Clock(#[source] std::time::SystemTimeError),

    #[error("时间戳溢出")]
    TimestampOverflow,

    #[error("字段 {field} 超出可存储范围: {value}")]
    CountOverflow { field: &'static str, value: usize },

    #[error("IO 操作失败: {0}")]
    Io(#[source] std::io::Error),

    #[error("统计结果异常，表 {table} 出现负数计数: {count}")]
    NegativeCount { table: &'static str, count: i64 },

    #[error("缺少 catalog 记录: {file_path}")]
    MissingCatalogEntry { file_path: String },
}
