import type {
  EnterprisePolicyDto,
  FontPreset,
  FontScale,
  IndexFilterConfigDto,
  IndexingMode,
  McpSettingsDto,
  MemorySettingsDto,
  ModelSettingsDto,
  ResourceBudget,
  ThemeMode
} from "../components/settings/types";
import type { Language } from "../i18n";
import type { AppSettingsDto, VaultStats, VaultStatsRaw } from "./types";

import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

export const TAURI_HOST_MISSING_MESSAGE = "未检测到 Tauri 宿主环境，请使用 cargo tauri dev 启动";
export const AI_LANG_STORAGE_KEY = "memori-ai-language";
export const THEME_STORAGE_KEY = "memori-theme";
export const LEGACY_THEME_MODE_STORAGE_KEY = "memori-theme-mode";
export const FONT_PRESET_STORAGE_KEY = "memori-font-preset";
export const FONT_SCALE_STORAGE_KEY = "memori-font-scale";
export const RETRIEVE_TOP_K_STORAGE_KEY = "memori-retrieve-top-k";
export const MODEL_ACTION_TIMEOUT_MS = 20000;
export const LOCAL_MODEL_ACTION_TIMEOUT_MS = 45000;
export const INDEXING_ACTION_TIMEOUT_MS = 15000;
export const DEFAULT_FONT_SCALE: FontScale = "m";
export const MODEL_NOT_CONFIGURED_CODE = "model_not_configured";
export const MARKDOWN_REMARK_PLUGINS = [remarkGfm, remarkBreaks];
export const MARKDOWN_REHYPE_PLUGINS = [rehypeRaw, rehypeSanitize, rehypeHighlight];

export const SIDEBAR_WIDTH_STORAGE_KEY = "memori-sidebar-width";
export const DEFAULT_SIDEBAR_WIDTH = 220;
export const MIN_SIDEBAR_WIDTH = 160;
export const MAX_SIDEBAR_WIDTH = 480;

export const DEFAULT_MODEL_SETTINGS: ModelSettingsDto = {
  active_provider: "llama_cpp_local",
  local_profile: {
    chat_endpoint: "http://localhost:18001",
    graph_endpoint: "http://localhost:18002",
    embed_endpoint: "http://localhost:18003",
    models_root: "",
    llama_server_path: "",
    chat_model: "qwen3-14b",
    graph_model: "qwen3-8b",
    embed_model: "Qwen3-Embedding-4B",
    chat_model_path: "",
    graph_model_path: "",
    embed_model_path: ""
  },
  remote_profile: {
    chat_endpoint: "https://api.openai.com",
    graph_endpoint: "https://api.openai.com",
    embed_endpoint: "https://api.openai.com",
    api_key: "",
    chat_model: "gpt-4o-mini",
    graph_model: "gpt-4o-mini",
    embed_model: "text-embedding-3-small"
  },
  stop_local_models_on_exit: true
};

export const DEFAULT_ENTERPRISE_POLICY: EnterprisePolicyDto = {
  egress_mode: "local_only",
  allowed_model_endpoints: [],
  allowed_models: []
};

export const DEFAULT_MCP_SETTINGS: McpSettingsDto = {
  enabled: false,
  transports: ["http", "stdio"],
  http_bind: "127.0.0.1",
  http_port: 3757,
  access_mode: "full_control",
  audit_enabled: true
};

export const DEFAULT_MEMORY_SETTINGS: MemorySettingsDto = {
  conversation_memory_enabled: true,
  auto_memory_write: "suggest",
  memory_write_requires_source: true,
  memory_markdown_export_enabled: false,
  default_context_budget: "16k",
  complex_context_budget: "32k",
  graph_ranking_enabled: false
};

export const DEFAULT_FILTER_CONFIG: IndexFilterConfigDto = {
  enabled: false,
  include_extensions: [],
  exclude_extensions: [],
  exclude_paths: [],
  include_paths: [],
  min_mtime: null,
  max_mtime: null,
  min_size: null,
  max_size: null
};

export function detectDefaultLanguage(): Language {
  if (typeof navigator === "undefined") {
    return "en-US";
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export function resolveInitialAiLanguage(): Language {
  if (typeof window === "undefined") {
    return "en-US";
  }
  const saved = window.localStorage.getItem(AI_LANG_STORAGE_KEY);
  if (saved === "zh-CN" || saved === "en-US") {
    return saved;
  }
  return detectDefaultLanguage();
}

export function resolveInitialThemeMode(): ThemeMode {
  if (typeof window === "undefined") {
    return "dark";
  }
  const saved = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (saved === "dark" || saved === "light") {
    return saved;
  }
  const legacy = window.localStorage.getItem(LEGACY_THEME_MODE_STORAGE_KEY);
  if (legacy === "a" || legacy === "b") {
    return "dark";
  }
  return "light";
}

export function resolveInitialFontPreset(): FontPreset {
  if (typeof window === "undefined") {
    return "system";
  }
  const saved = window.localStorage.getItem(FONT_PRESET_STORAGE_KEY);
  if (saved === "neo" || saved === "mono" || saved === "system") {
    return saved;
  }
  return "system";
}

export function resolveInitialFontScale(): FontScale {
  if (typeof window === "undefined") {
    return DEFAULT_FONT_SCALE;
  }
  const saved = window.localStorage.getItem(FONT_SCALE_STORAGE_KEY);
  if (saved === "s" || saved === "l" || saved === "m") {
    return saved;
  }
  return DEFAULT_FONT_SCALE;
}

export function resolveInitialRetrieveTopK(): number {
  if (typeof window === "undefined") {
    return 20;
  }
  const saved = window.localStorage.getItem(RETRIEVE_TOP_K_STORAGE_KEY);
  const parsed = Number.parseInt(saved ?? "", 10);
  if (Number.isFinite(parsed) && parsed >= 1 && parsed <= 50) {
    return parsed;
  }
  return 20;
}

export function resolveInitialSidebarWidth(): number {
  if (typeof window === "undefined") {
    return DEFAULT_SIDEBAR_WIDTH;
  }
  const saved = window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY);
  const parsed = Number.parseInt(saved ?? "", 10);
  if (Number.isFinite(parsed) && parsed >= MIN_SIDEBAR_WIDTH && parsed <= MAX_SIDEBAR_WIDTH) {
    return parsed;
  }
  return DEFAULT_SIDEBAR_WIDTH;
}

export function normalizeIndexingMode(value: string | null | undefined): IndexingMode {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "manual") return "manual";
  if (normalized === "scheduled") return "scheduled";
  return "continuous";
}

export function normalizeResourceBudget(value: string | null | undefined): ResourceBudget {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "balanced") return "balanced";
  if (normalized === "fast") return "fast";
  return "low";
}

export function normalizeContextBudget(value: string | null | undefined, fallback: string): string {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "8k" || normalized === "16k" || normalized === "32k" || normalized === "64k") {
    return normalized;
  }
  return fallback;
}

export function settingsToMemorySettings(settings: AppSettingsDto): MemorySettingsDto {
  return {
    conversation_memory_enabled: settings.conversation_memory_enabled ?? true,
    auto_memory_write: settings.auto_memory_write || "suggest",
    memory_write_requires_source: settings.memory_write_requires_source ?? true,
    memory_markdown_export_enabled: false,
    default_context_budget: normalizeContextBudget(settings.default_context_budget, "16k"),
    complex_context_budget: normalizeContextBudget(settings.complex_context_budget, "32k"),
    graph_ranking_enabled: false
  };
}

export function isTauriHostAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const w = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

export function toUiErrorMessage(error: unknown): string {
  if (!isTauriHostAvailable()) {
    return TAURI_HOST_MISSING_MESSAGE;
  }

  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (error && typeof error === "object") {
    const maybeMessage = (error as { message?: unknown }).message;
    if (typeof maybeMessage === "string" && maybeMessage.trim()) {
      return maybeMessage;
    }
  }

  return "Backend command failed. Check desktop logs.";
}

export function withTimeout<T>(promise: Promise<T>, timeoutMs: number, timeoutMessage: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(timeoutMessage));
    }, timeoutMs);
    promise
      .then((value) => {
        window.clearTimeout(timer);
        resolve(value);
      })
      .catch((error) => {
        window.clearTimeout(timer);
        reject(error);
      });
  });
}

export function normalizeStats(raw: VaultStatsRaw): VaultStats {
  return {
    documents: raw.documents ?? raw.document_count ?? 0,
    chunks: raw.chunks ?? raw.chunk_count ?? 0,
    nodes: raw.nodes ?? raw.graph_node_count ?? 0
  };
}
