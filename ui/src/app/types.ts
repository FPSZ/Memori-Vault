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

export type SourceGroup = {
  group_id: string;
  canonical_title: string;
  file_paths: string[];
  relative_paths: string[];
  citation_indices: number[];
  evidence_count: number;
};

export type MemoryEvidence = {
  id: number;
  layer: "stm" | "mtm" | "ltm" | "graph" | "policy";
  scope: "user" | "project" | "session" | "agent" | "document";
  memory_type: string;
  title: string;
  content: string;
  source_type: "document_chunk" | "conversation_turn" | "tool_event" | "system_event" | "markdown_note" | "graph_edge";
  source_ref: string;
  confidence: number;
  status: "active" | "pending" | "superseded" | "deleted";
};

export type ContextBudgetReport = {
  token_budget: number;
  used_by_documents: number;
  used_by_memory: number;
  used_by_graph: number;
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
  answer_source_mix?: "document_only" | "document_plus_memory" | "memory_only" | "insufficient";
  memory_context?: MemoryEvidence[];
  source_groups?: SourceGroup[];
  failure_class?: "recall_miss" | "rank_miss" | "gating_false_negative" | "generation_refusal" | "citation_miss" | "none";
  context_budget_report?: ContextBudgetReport;
};

export type AppSettingsDto = {
  watch_root: string;
  language?: string | null;
  indexing_mode?: string | null;
  resource_budget?: string | null;
  schedule_start?: string | null;
  schedule_end?: string | null;
  conversation_memory_enabled?: boolean;
  auto_memory_write?: "off" | "suggest" | "auto_low_risk" | string;
  memory_write_requires_source?: boolean;
  memory_markdown_export_enabled?: boolean;
  default_context_budget?: string;
  complex_context_budget?: string;
  graph_ranking_enabled?: boolean;
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
