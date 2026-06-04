import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettingsDto,
  AskResponseStructured,
  FileMatch,
  FilePreviewDto,
  SearchScopeItem,
  VaultStatsRaw
} from "../types";
import type {
  EnterprisePolicyDto,
  IndexFilterConfigDto,
  IndexingStatusDto,
  McpSettingsDto,
  McpStatusDto,
  MemorySettingsDto,
  ModelAvailabilityDto,
  ModelProvider,
  RemoteApiFormat,
  ModelSettingsDto,
  LocalModelRuntimeStatusesDto,
  ProviderModelsDto
} from "../../components/settings/types";
import type {
  GraphNodeDto,
  GraphNeighborsDto,
  GraphStatsDto
} from "../types";

type ProviderModelsPayload = {
  provider: ModelProvider;
  chatEndpoint: string;
  graphEndpoint: string;
  embedEndpoint: string;
  apiKey: string | null;
  modelsRoot: string | null;
};

export function askVaultStructured(payload: {
  query: string;
  lang: "zh-CN" | "en-US";
  topK: number;
  scopePaths: string[];
}) {
  return invoke<AskResponseStructured>("ask_vault_structured", payload);
}

export function rankSettingsQuery(payload: {
  query: string;
  candidates: Array<{ key: string; text: string }>;
  lang: "zh-CN" | "en-US";
}) {
  return invoke<string[]>("rank_settings_query", payload);
}

export function getVaultStats() {
  return invoke<VaultStatsRaw>("get_vault_stats");
}

export function getAppSettings() {
  return invoke<AppSettingsDto>("get_app_settings");
}

export function getIndexingStatus() {
  return invoke<IndexingStatusDto>("get_indexing_status");
}

export function setIndexingMode(payload: {
  indexing_mode: string;
  resource_budget: string;
  schedule_start: string | null;
  schedule_end: string | null;
}) {
  return invoke<AppSettingsDto>("set_indexing_mode", { payload });
}

export function triggerReindex() {
  return invoke<string>("trigger_reindex");
}

export function pauseIndexing() {
  return invoke("pause_indexing");
}

export function resumeIndexing() {
  return invoke("resume_indexing");
}

export function getIndexFilter() {
  return invoke<IndexFilterConfigDto | null>("get_index_filter");
}

export function setIndexFilter(payload: IndexFilterConfigDto) {
  return invoke<AppSettingsDto>("set_index_filter", { payload });
}

export function getModelSettings() {
  return invoke<ModelSettingsDto>("get_model_settings");
}

export function setModelSettings(payload: ModelSettingsDto) {
  return invoke<ModelSettingsDto>("set_model_settings", { payload });
}

export function listProviderModels(payload: ProviderModelsPayload) {
  return invoke<ProviderModelsDto>("list_provider_models", payload);
}

export function getEnterprisePolicy() {
  return invoke<EnterprisePolicyDto>("get_enterprise_policy");
}

export function setEnterprisePolicy(payload: EnterprisePolicyDto) {
  return invoke<EnterprisePolicyDto>("set_enterprise_policy", { payload });
}

export function validateModelSetup() {
  return invoke<ModelAvailabilityDto>("validate_model_setup");
}

export function getLocalModelRuntimeStatus() {
  return invoke<LocalModelRuntimeStatusesDto>("get_local_model_runtime_status");
}

export function startLocalModel(role: "chat" | "graph" | "embed" | "rerank") {
  return invoke<LocalModelRuntimeStatusesDto>("start_local_model", { role });
}

export function stopLocalModel(role: "chat" | "graph" | "embed" | "rerank") {
  return invoke<LocalModelRuntimeStatusesDto>("stop_local_model", { role });
}

export function restartLocalModel(role: "chat" | "graph" | "embed" | "rerank") {
  return invoke<LocalModelRuntimeStatusesDto>("restart_local_model", { role });
}

export function downloadRerankModel() {
  return invoke<string>("download_rerank_model");
}

export function probeModelProvider(payload: ProviderModelsPayload) {
  return invoke<ModelAvailabilityDto>("probe_model_provider", payload);
}

export function testRemoteConnection(payload: {
  baseUrl: string;
  apiKey: string | null;
  apiFormat: RemoteApiFormat;
  chatModel: string;
}) {
  return invoke<string>("test_remote_connection", payload);
}

export function listSearchScopes() {
  return invoke<SearchScopeItem[]>("list_search_scopes");
}

export function searchFiles(payload: {
  query: string;
  limit: number;
  scopePaths?: string[] | undefined;
}) {
  return invoke<FileMatch[]>("search_files", payload);
}

export function openSourceLocation(path: string) {
  return invoke("open_source_location", { path });
}

export function readFileContent(path: string) {
  return invoke<string>("read_file_content", { path });
}

export function readFilePreview(path: string) {
  return invoke<FilePreviewDto>("read_file_preview", { path });
}

export function getMcpSettings() {
  return invoke<McpSettingsDto>("get_mcp_settings");
}

export function setMcpSettings(payload: McpSettingsDto) {
  return invoke<McpSettingsDto>("set_mcp_settings", { payload });
}

export function setMemorySettings(payload: MemorySettingsDto) {
  return invoke<AppSettingsDto>("set_memory_settings", { payload });
}

export function getMcpStatus() {
  return invoke<McpStatusDto>("get_mcp_status");
}

export function copyMcpClientConfig(client: string) {
  return invoke<string>("copy_mcp_client_config", { client });
}

export function setWatchRoot(path: string) {
  return invoke<AppSettingsDto>("set_watch_root", { path });
}

export type LogEntry = {
  timestamp: string;
  level: string;
  category: string;
  target: string;
  message: string;
  file: string | null;
  line: number | null;
  thread_id: string | null;
};

export function getLogs(payload: { limit?: number; level_filter?: string | null }) {
  return invoke<LogEntry[]>("get_logs", {
    limit: payload.limit,
    levelFilter: payload.level_filter ?? null
  });
}

export function getLogDir() {
  return invoke<string>("get_log_dir");
}

export function searchGraphNodes(query: string, limit?: number) {
  return invoke<GraphNodeDto[]>("search_graph_nodes", { query, limit });
}

export function getGraphNeighbors(entityId: string, limit?: number) {
  return invoke<GraphNeighborsDto>("get_graph_neighbors", { entityId, limit });
}

export function getGraphStats() {
  return invoke<GraphStatsDto>("get_graph_stats");
}

export type RetrievalRegressionSummary = {
  case_count: number;
  answer_cases: number;
  refuse_cases: number;
  top1_document_hit_rate: number;
  top1_chunk_hit_rate: number;
  top3_document_recall_rate: number;
  top5_chunk_recall_rate: number;
  chunk_mrr: number;
  citation_validity_rate: number;
  reject_correctness_rate: number;
  rerank_applied_rate: number;
};

export type RetrievalRegressionCase = {
  id: string;
  query: string;
  mode: "answer" | "refuse";
  status: string;
  timed_out: boolean;
  scope_paths: string[];
  target_documents: string[];
  target_clues: string[];
  document_hit_rank: number | null;
  chunk_hit_rank: number | null;
  top1_document_hit: boolean;
  top1_chunk_hit: boolean;
  top3_document_recall: boolean;
  top5_chunk_recall: boolean;
  citation_valid: boolean;
  reject_correct: boolean;
  rerank_applied: boolean;
  citations_count: number;
  final_evidence_count: number;
  gating_decision_reason: string;
  doc_recall_ms: number;
  doc_dense_ms: number;
  chunk_lexical_ms: number;
  chunk_dense_ms: number;
  merge_ms: number;
  rerank_ms: number;
  notes: string | null;
};

export type RetrievalRegressionReportEntry = {
  id: string;
  name: string;
  path: string;
  json_path: string;
  md_path: string | null;
  mode: string;
  profile: string;
  generated_at_utc: string;
  service_health: string;
  rerank_health: string;
  case_count: number;
  last_modified_ms: number;
  summary: RetrievalRegressionSummary;
};

export type RetrievalRegressionReport = {
  tool: string;
  generated_at_utc: string;
  evaluation_mode: string;
  profile: string;
  suite_path: string;
  watch_root: string;
  db_path: string;
  baseline: unknown;
  service_health: string;
  rerank_health: string;
  live_service_used: boolean;
  index_prep_ms: number | null;
  case_timeout_count: number;
  preparation_error: string | null;
  summary: RetrievalRegressionSummary;
  cases: RetrievalRegressionCase[];
};

export type RunRetrievalRegressionPayload = {
  mode: "offline_deterministic" | "live_embedding";
  profile: "core_docs" | "repo_mixed" | "full_live";
  suite?: string;
  watchRoot?: string;
  dbPath?: string;
  caseFilter?: string;
  maxIndexPrepSecs?: number;
  maxCaseSecs?: number;
};

export type RetrievalRegressionRunState = {
  id: string;
  status: "running" | "succeeded" | "failed";
  mode: string;
  profile: string;
  case_filter: string | null;
  started_at_ms: number;
  finished_at_ms: number | null;
  exit_code: number | null;
  report_path: string | null;
  stdout_tail: string;
  stderr_tail: string;
  error: string | null;
};

// Live, per-case progress emitted by the regression harness while a run is in
// flight. The backend (memori-core example + Tauri command) writes this after
// each case so the UI can show the current question, current phase, and how
// many cases remain. Until the backend command exists the getter rejects and
// the UI degrades to plain run status — so treat every field as best-effort.
export type RetrievalRegressionProgress = {
  run_id: string;
  status: "running" | "succeeded" | "failed" | "idle";
  total: number;
  completed: number;
  current_index: number; // 1-based index of the case in flight; 0 when none
  current_case_id: string;
  current_query: string;
  current_mode: string; // "answer" | "refuse" | ""
  current_phase: string; // preparing | doc_recall | chunk_recall | rerank | gating | scoring | done
  passed: number;
  failed: number;
  updated_at_ms: number;
};

export function listRetrievalRegressionReports() {
  return invoke<RetrievalRegressionReportEntry[]>("list_retrieval_regression_reports");
}

export function readRetrievalRegressionReport(reportPath: string) {
  return invoke<RetrievalRegressionReport>("read_retrieval_regression_report", {
    reportPath
  });
}

export function runRetrievalRegression(payload: RunRetrievalRegressionPayload) {
  return invoke<RetrievalRegressionRunState>("run_retrieval_regression", { payload });
}

export function getRetrievalRegressionRun(runId?: string) {
  return invoke<RetrievalRegressionRunState | null>("get_retrieval_regression_run", {
    runId: runId ?? null
  });
}

export function getRetrievalRegressionProgress(runId?: string) {
  return invoke<RetrievalRegressionProgress | null>("get_retrieval_regression_progress", {
    runId: runId ?? null
  });
}
