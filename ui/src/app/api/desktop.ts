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

export function startLocalModel(role: "chat" | "graph" | "embed") {
  return invoke<LocalModelRuntimeStatusesDto>("start_local_model", { role });
}

export function stopLocalModel(role: "chat" | "graph" | "embed") {
  return invoke<LocalModelRuntimeStatusesDto>("stop_local_model", { role });
}

export function restartLocalModel(role: "chat" | "graph" | "embed") {
  return invoke<LocalModelRuntimeStatusesDto>("restart_local_model", { role });
}

export function probeModelProvider(payload: ProviderModelsPayload) {
  return invoke<ModelAvailabilityDto>("probe_model_provider", payload);
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
