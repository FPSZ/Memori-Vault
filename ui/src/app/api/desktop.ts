import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettingsDto,
  AskResponseStructured,
  FileMatch,
  SearchScopeItem,
  VaultStatsRaw
} from "../types";
import type {
  EnterprisePolicyDto,
  IndexingStatusDto,
  McpSettingsDto,
  McpStatusDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto
} from "../../components/settings/types";

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

export function probeModelProvider(payload: ProviderModelsPayload) {
  return invoke<ModelAvailabilityDto>("probe_model_provider", payload);
}

export function pullModel(payload: {
  model: string;
  provider: ModelProvider;
  endpoint: string;
  apiKey: string | null;
}) {
  return invoke<ModelAvailabilityDto>("pull_model", payload);
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

export function getMcpSettings() {
  return invoke<McpSettingsDto>("get_mcp_settings");
}

export function setMcpSettings(payload: McpSettingsDto) {
  return invoke<McpSettingsDto>("set_mcp_settings", { payload });
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
