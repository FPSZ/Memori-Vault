import { useI18n } from "../i18n";

export type Translate = ReturnType<typeof useI18n>["t"];

export type VaultStats = {
  documents: number;
  chunks: number;
  nodes: number;
};

export type VaultStatsRaw = Partial<
  VaultStats & {
    document_count: number;
    chunk_count: number;
    graph_node_count: number;
  }
>;

export type AskStatus = "answered" | "insufficient_evidence" | "model_failed_with_evidence";

export type CitationItem = {
  index: number;
  file_path: string;
  relative_path: string;
  chunk_index: number;
  heading_path: string[];
  excerpt: string;
};

export type VisibleCitation = CitationItem & {
  citation_key: string;
  duplicate_count: number;
  is_long_excerpt: boolean;
};

export type VisibleEvidenceGroup = {
  evidence_key: string;
  file_path: string;
  relative_path: string;
  heading_paths: string[];
  block_kinds: string[];
  document_reasons: string[];
  reasons: string[];
  document_rank: number;
  top_chunk_rank: number;
  chunk_ranks: number[];
  content: string;
  fragment_count: number;
};

export type MetricRow = {
  key: string;
  label: string;
  value: number;
};

export type EvidenceItem = {
  file_path: string;
  relative_path: string;
  chunk_index: number;
  heading_path: string[];
  block_kind: string;
  document_reason: "lexical" | "filename" | "both" | "scope" | string;
  reason: "lexical" | "dense" | "both" | "unknown" | string;
  document_rank: number;
  chunk_rank: number;
  document_raw_score?: number | null;
  lexical_raw_score?: number | null;
  dense_raw_score?: number | null;
  final_score: number;
  content: string;
};

export type RetrievalMetrics = {
  query_analysis_ms: number;
  doc_recall_ms: number;
  doc_lexical_ms: number;
  doc_merge_ms: number;
  chunk_lexical_ms: number;
  chunk_dense_ms: number;
  merge_ms: number;
  answer_ms: number;
  doc_candidate_count: number;
  chunk_candidate_count: number;
  final_evidence_count: number;
  top_doc_distinct_term_hits?: number;
  top_doc_term_coverage?: number;
  gating_decision_reason?: string;
  docs_phrase_quality?: string;
  query_flags: string[];
};

export type AskResponseStructured = {
  status: AskStatus;
  answer: string;
  question: string;
  scope_paths: string[];
  citations: CitationItem[];
  evidence: EvidenceItem[];
  metrics: RetrievalMetrics;
};

export type AppSettingsDto = {
  watch_root: string;
  language?: string | null;
  indexing_mode?: string | null;
  resource_budget?: string | null;
  schedule_start?: string | null;
  schedule_end?: string | null;
};

export type SearchScopeItem = {
  path: string;
  name: string;
  relative_path: string;
  is_dir: boolean;
  depth: number;
};

export type FileMatch = {
  file_path: string;
  file_name: string;
  parent_dir: string;
  ext: string;
  mtime_secs: number;
  file_size: number;
};
