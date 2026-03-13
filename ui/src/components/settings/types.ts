import type { Language } from "../../i18n";

export type FontPreset = "system" | "neo" | "mono";
export type FontScale = "s" | "m" | "l";
export type ThemeMode = "dark" | "light";
export type ModelProvider = "ollama_local" | "openai_compatible";
export type IndexingMode = "continuous" | "manual" | "scheduled";
export type ResourceBudget = "low" | "balanced" | "fast";
export type ModelRole = "chat_model" | "graph_model" | "embed_model";

export type LocalModelProfileDto = {
  endpoint: string;
  models_root?: string | null;
  chat_model: string;
  graph_model: string;
  embed_model: string;
};

export type RemoteModelProfileDto = {
  endpoint: string;
  api_key?: string | null;
  chat_model: string;
  graph_model: string;
  embed_model: string;
};

export type ModelSettingsDto = {
  active_provider: ModelProvider;
  local_profile: LocalModelProfileDto;
  remote_profile: RemoteModelProfileDto;
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
  onPullModel: (model: string) => Promise<void>;
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
};
