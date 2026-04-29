import type { Language } from "../../i18n";

export type FontPreset = "system" | "neo" | "mono";
export type FontScale = "s" | "m" | "l";
export type ThemeMode = "dark" | "light";
export type ModelProvider = "llama_cpp_local" | "openai_compatible";
export type LocalPerformancePreset = "compat" | "gpu" | "low_vram" | "throughput";
export type IndexingMode = "continuous" | "manual" | "scheduled";
export type ResourceBudget = "low" | "balanced" | "fast";
export type ModelRole = "chat_model" | "graph_model" | "embed_model";
export type McpTransport = "stdio" | "http";
export type McpTransportMode = "stdio" | "http" | "both";

export type IndexFilterConfigDto = {
  enabled: boolean;
  include_extensions: string[];
  exclude_extensions: string[];
  exclude_paths: string[];
  include_paths: string[];
  min_mtime: string | null;
  max_mtime: string | null;
  min_size: number | null;
  max_size: number | null;
};

export type LocalModelProfileDto = {
  chat_endpoint: string;
  graph_endpoint: string;
  embed_endpoint: string;
  models_root?: string | null;
  llama_server_path?: string | null;
  chat_model: string;
  graph_model: string;
  embed_model: string;
  chat_model_path?: string | null;
  graph_model_path?: string | null;
  embed_model_path?: string | null;
  chat_context_length?: number | null;
  graph_context_length?: number | null;
  embed_context_length?: number | null;
  chat_concurrency?: number | null;
  graph_concurrency?: number | null;
  embed_concurrency?: number | null;
  performance_preset?: LocalPerformancePreset | string | null;
  n_gpu_layers?: number | null;
  batch_size?: number | null;
  ubatch_size?: number | null;
  threads?: number | null;
  threads_batch?: number | null;
  flash_attn?: boolean | null;
  cache_type_k?: string | null;
  cache_type_v?: string | null;
};

export type RemoteModelProfileDto = {
  chat_endpoint: string;
  graph_endpoint: string;
  embed_endpoint: string;
  api_key?: string | null;
  chat_model: string;
  graph_model: string;
  embed_model: string;
  chat_context_length?: number | null;
  graph_context_length?: number | null;
  embed_context_length?: number | null;
  chat_concurrency?: number | null;
  graph_concurrency?: number | null;
  embed_concurrency?: number | null;
};

export type ModelSettingsDto = {
  active_provider: ModelProvider;
  local_profile: LocalModelProfileDto;
  remote_profile: RemoteModelProfileDto;
};

export type LocalModelRuntimeStatusDto = {
  role: "chat" | "graph" | "embed" | string;
  endpoint: string;
  port?: number | null;
  pid?: number | null;
  state: "stopped" | "running" | "error" | string;
  message?: string | null;
};

export type LocalModelRuntimeStatusesDto = {
  roles: LocalModelRuntimeStatusDto[];
};

export type EnterprisePolicyDto = {
  egress_mode: "local_only" | "allowlist";
  allowed_model_endpoints: string[];
  allowed_models: string[];
};

export type ProviderModelsDto = {
  from_folder: string[];
  from_service: string[];
  merged: string[];
};

export type ModelAvailabilityDto = {
  configured: boolean;
  reachable: boolean;
  models: string[];
  missing_roles: string[];
  errors: Array<{ code: string; message: string }>;
  checked_provider?: string | null;
  status_code?: string | null;
  status_message?: string | null;
};

export type IndexingStatusDto = {
  phase: string;
  indexed_docs: number;
  indexed_chunks: number;
  graphed_chunks: number;
  graph_backlog: number;
  total_docs: number;
  total_chunks: number;
  progress_percent: number;
  last_scan_at?: number | null;
  last_error?: string | null;
  paused: boolean;
  mode: string;
  resource_budget: string;
  rebuild_state: string;
  rebuild_reason?: string | null;
  index_format_version: number;
  parser_format_version: number;
};

export type McpSettingsDto = {
  enabled: boolean;
  transports: McpTransport[];
  http_bind: string;
  http_port: number;
  access_mode: "full_control" | "read_only";
  audit_enabled: boolean;
};

export type McpStatusDto = {
  enabled: boolean;
  protocol_version: string;
  http_endpoint: string;
  stdio_command: string;
  tools_count: number;
  resources_count: number;
  prompts_count: number;
};

export type MemorySettingsDto = {
  conversation_memory_enabled: boolean;
  auto_memory_write: "off" | "suggest" | "auto_low_risk" | string;
  memory_write_requires_source: boolean;
  memory_markdown_export_enabled: boolean;
  default_context_budget: string;
  complex_context_budget: string;
  graph_ranking_enabled: boolean;
};

export type SettingsModalProps = {
  open: boolean;
  onBack: () => void;
  uiLang: Language;
  aiLang: Language;
  onUiLangChange: (lang: Language) => void;
  onAiLangChange: (lang: Language) => void;
  watchRoot: string;
  isPickingWatchRoot: boolean;
  onPickWatchRoot: () => void;
  retrieveTopK: number;
  onRetrieveTopKChange: (value: number) => void;
  fontPreset: FontPreset;
  onFontPresetChange: (preset: FontPreset) => void;
  fontScale: FontScale;
  onFontScaleChange: (scale: FontScale) => void;
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
  modelSettings: ModelSettingsDto;
  enterprisePolicy: EnterprisePolicyDto;
  modelAvailability: ModelAvailabilityDto | null;
  providerModels: ProviderModelsDto;
  modelBusy: boolean;
  enterpriseBusy: boolean;
  onModelSettingsChange: (next: ModelSettingsDto) => void;
  onEnterprisePolicyChange: (next: EnterprisePolicyDto) => void;
  onSaveModelSettings: () => Promise<void>;
  onSaveEnterprisePolicy: () => Promise<void>;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
  localModelRuntimeStatuses: LocalModelRuntimeStatusesDto | null;
  localModelRuntimeBusyRole: string | null;
  onRefreshLocalModelRuntimeStatus: () => Promise<void>;
  onStartLocalModel: (role: "chat" | "graph" | "embed") => Promise<void>;
  onStopLocalModel: (role: "chat" | "graph" | "embed") => Promise<void>;
  onRestartLocalModel: (role: "chat" | "graph" | "embed") => Promise<void>;
  onPickLocalModelsRoot: () => Promise<void>;
  onClearLocalModelsRoot: () => void;
  indexingMode: IndexingMode;
  resourceBudget: ResourceBudget;
  scheduleStart: string;
  scheduleEnd: string;
  indexingStatus: IndexingStatusDto | null;
  indexingBusy: boolean;
  onIndexingModeChange: (mode: IndexingMode) => void;
  onResourceBudgetChange: (budget: ResourceBudget) => void;
  onScheduleStartChange: (value: string) => void;
  onScheduleEndChange: (value: string) => void;
  onSaveIndexingConfig: () => Promise<void>;
  onTriggerReindex: () => Promise<void>;
  onPauseIndexing: () => Promise<void>;
  onResumeIndexing: () => Promise<void>;
  mcpSettings: McpSettingsDto;
  mcpStatus: McpStatusDto | null;
  mcpBusy: boolean;
  mcpMessage: string | null;
  onMcpSettingsChange: (next: McpSettingsDto) => void;
  onSaveMcpSettings: () => Promise<void>;
  onCopyMcpClientConfig: (client: string) => Promise<void>;
  memorySettings: MemorySettingsDto;
  memoryBusy: boolean;
  memoryMessage: string | null;
  onMemorySettingsChange: (next: MemorySettingsDto) => void;
  onSaveMemorySettings: () => Promise<void>;
  filterConfig: IndexFilterConfigDto;
  filterBusy: boolean;
  filterMessage: string | null;
  onFilterConfigChange: (next: IndexFilterConfigDto) => void;
  onSaveFilterConfig: () => Promise<void>;
};
