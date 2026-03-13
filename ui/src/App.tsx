import {
  KeyboardEvent as ReactKeyboardEvent,
  UIEvent as ReactUIEvent,
  WheelEvent as ReactWheelEvent,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "framer-motion";
import {
  Atom,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  FileText,
  FolderOpen,
  LoaderCircle,
  Search,
} from "lucide-react";
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import {
  SettingsModal
} from "./components/SettingsModal";
import type {
  EnterprisePolicyDto,
  FontPreset,
  FontScale,
  IndexingMode,
  IndexingStatusDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto,
  ResourceBudget,
  ThemeMode
} from "./components/settings/types";
import { fadeSlideUpVariants } from "./components/MotionKit";
import { useI18n } from "./i18n";
import type { Language } from "./i18n";
import {
  getAppSettings,
  getEnterprisePolicy,
  getIndexingStatus,
  getModelSettings,
  getVaultStats,
  listProviderModels,
  listSearchScopes,
  openSourceLocation,
  pauseIndexing,
  probeModelProvider,
  pullModel,
  resumeIndexing,
  setEnterprisePolicy as saveEnterprisePolicyRemote,
  setIndexingMode as saveIndexingModeRemote,
  setModelSettings as saveModelSettingsRemote,
  setWatchRoot as saveWatchRootRemote,
  searchFiles,
  triggerReindex,
  validateModelSetup
} from "./app/api/desktop";
import { buildVisibleCitations, buildVisibleEvidenceGroups } from "./app/evidence";
import {
  buildCollapsedMarkdownPreview,
  formatElapsed,
  formatMetricDuration,
  normalizeScopeKey,
  statusToneClasses
} from "./app/formatters";
import { useQueryFlow } from "./app/hooks/useQueryFlow";
import { OnboardingOverlay } from "./app/layout/OnboardingOverlay";
import { ResultStage } from "./app/layout/ResultStage";
import { SearchStage } from "./app/layout/SearchStage";
import { StatusFooter } from "./app/layout/StatusFooter";
import { TitleBar } from "./app/layout/TitleBar";
import type {
  AppSettingsDto,
  AskResponseStructured,
  FileMatch,
  MetricRow,
  SearchScopeItem,
  VaultStats,
  VaultStatsRaw,
  VisibleCitation,
  VisibleEvidenceGroup
} from "./app/types";

const TAURI_HOST_MISSING_MESSAGE = "未检测到 Tauri 宿主环境，请使用 cargo tauri dev 启动";
const AI_LANG_STORAGE_KEY = "memori-ai-language";
const THEME_STORAGE_KEY = "memori-theme";
const LEGACY_THEME_MODE_STORAGE_KEY = "memori-theme-mode";
const FONT_PRESET_STORAGE_KEY = "memori-font-preset";
const FONT_SCALE_STORAGE_KEY = "memori-font-scale";
const RETRIEVE_TOP_K_STORAGE_KEY = "memori-retrieve-top-k";
const MODEL_ACTION_TIMEOUT_MS = 20000;
const INDEXING_ACTION_TIMEOUT_MS = 15000;
const DEFAULT_FONT_SCALE: FontScale = "m";
const MARKDOWN_REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const MARKDOWN_REHYPE_PLUGINS = [rehypeRaw, rehypeSanitize, rehypeHighlight];
const MODEL_NOT_CONFIGURED_CODE = "model_not_configured";

const DEFAULT_MODEL_SETTINGS: ModelSettingsDto = {
  active_provider: "ollama_local",
  local_profile: {
    endpoint: "http://localhost:11434",
    models_root: "",
    chat_model: "qwen2.5:7b",
    graph_model: "qwen2.5:7b",
    embed_model: "nomic-embed-text:latest"
  },
  remote_profile: {
    endpoint: "https://api.openai.com",
    api_key: "",
    chat_model: "gpt-4o-mini",
    graph_model: "gpt-4o-mini",
    embed_model: "text-embedding-3-small"
  }
};

const DEFAULT_ENTERPRISE_POLICY: EnterprisePolicyDto = {
  egress_mode: "local_only",
  allowed_model_endpoints: [],
  allowed_models: []
};

function detectDefaultLanguage(): Language {
  if (typeof navigator === "undefined") {
    return "en-US";
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

function resolveInitialAiLanguage(): Language {
  if (typeof window === "undefined") {
    return "en-US";
  }
  const saved = window.localStorage.getItem(AI_LANG_STORAGE_KEY);
  if (saved === "zh-CN" || saved === "en-US") {
    return saved;
  }
  return detectDefaultLanguage();
}

function resolveInitialThemeMode(): ThemeMode {
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
  return "dark";
}

function resolveInitialFontPreset(): FontPreset {
  if (typeof window === "undefined") {
    return "system";
  }
  const saved = window.localStorage.getItem(FONT_PRESET_STORAGE_KEY);
  if (saved === "neo" || saved === "mono" || saved === "system") {
    return saved;
  }
  return "system";
}

function resolveInitialFontScale(): FontScale {
  if (typeof window === "undefined") {
    return DEFAULT_FONT_SCALE;
  }
  const saved = window.localStorage.getItem(FONT_SCALE_STORAGE_KEY);
  if (saved === "s" || saved === "l" || saved === "m") {
    return saved;
  }
  return DEFAULT_FONT_SCALE;
}

function resolveInitialRetrieveTopK(): number {
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

function normalizeIndexingMode(value: string | null | undefined): IndexingMode {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "manual") return "manual";
  if (normalized === "scheduled") return "scheduled";
  return "continuous";
}

function normalizeResourceBudget(value: string | null | undefined): ResourceBudget {
  const normalized = (value ?? "").trim().toLowerCase();
  if (normalized === "balanced") return "balanced";
  if (normalized === "fast") return "fast";
  return "low";
}

function isTauriHostAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const w = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

function toUiErrorMessage(error: unknown): string {
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

  return "调用后端命令失败，请检查桌面端日志。";
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, timeoutMessage: string): Promise<T> {
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

function normalizeStats(raw: VaultStatsRaw): VaultStats {
  return {
    documents: raw.documents ?? raw.document_count ?? 0,
    chunks: raw.chunks ?? raw.chunk_count ?? 0,
    nodes: raw.nodes ?? raw.graph_node_count ?? 0
  };
}


export default function App() {
  const { lang: uiLang, setLang: setUiLang, t } = useI18n();
  const [isSearchBarCompact, setIsSearchBarCompact] = useState(false);
  const [isSearchBarHovering, setIsSearchBarHovering] = useState(false);
  const [isSearchInputFocused, setIsSearchInputFocused] = useState(false);
  const [allowCompactHoverExpand, setAllowCompactHoverExpand] = useState(true);
  const [isMaximized, setIsMaximized] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isOnboardingOpen, setIsOnboardingOpen] = useState(false);
  const [onboardingStep, setOnboardingStep] = useState(0);
  const [aiLang, setAiLang] = useState<Language>(() => resolveInitialAiLanguage());
  const [themeMode, setThemeMode] = useState<ThemeMode>(() => resolveInitialThemeMode());
  const [fontPreset, setFontPreset] = useState<FontPreset>(() => resolveInitialFontPreset());
  const [fontScale, setFontScale] = useState<FontScale>(() => resolveInitialFontScale());
  const [retrieveTopK, setRetrieveTopK] = useState<number>(() => resolveInitialRetrieveTopK());
  const [watchRoot, setWatchRoot] = useState("");
  const [isPickingWatchRoot, setIsPickingWatchRoot] = useState(false);
  const [scopeMenuOpen, setScopeMenuOpen] = useState(false);
  const [scopeItems, setScopeItems] = useState<SearchScopeItem[]>([]);
  const [scopeLoading, setScopeLoading] = useState(false);
  const [fileMatches, setFileMatches] = useState<FileMatch[]>([]);
  const [fileMatchesOpen, setFileMatchesOpen] = useState(false);
  const [selectedScopePaths, setSelectedScopePaths] = useState<string[]>([]);
  const [expandedScopeDirs, setExpandedScopeDirs] = useState<Set<string>>(() => new Set());
  const [expandedSourceKeys, setExpandedSourceKeys] = useState<Set<string>>(() => new Set());
  const [expandedCitationKeys, setExpandedCitationKeys] = useState<Set<string>>(() => new Set());
  const [modelSettings, setModelSettings] = useState<ModelSettingsDto>(DEFAULT_MODEL_SETTINGS);
  const [enterprisePolicy, setEnterprisePolicy] =
    useState<EnterprisePolicyDto>(DEFAULT_ENTERPRISE_POLICY);
  const [modelAvailability, setModelAvailability] = useState<ModelAvailabilityDto | null>(null);
  const [providerModels, setProviderModels] = useState<ProviderModelsDto>({
    from_folder: [],
    from_service: [],
    merged: []
  });
  const [modelBusy, setModelBusy] = useState(false);
  const [enterpriseBusy, setEnterpriseBusy] = useState(false);
  const [indexingMode, setIndexingMode] = useState<IndexingMode>("continuous");
  const [resourceBudget, setResourceBudget] = useState<ResourceBudget>("low");
  const [scheduleStart, setScheduleStart] = useState("00:00");
  const [scheduleEnd, setScheduleEnd] = useState("06:00");
  const [indexingStatus, setIndexingStatus] = useState<IndexingStatusDto | null>(null);
  const [indexingBusy, setIndexingBusy] = useState(false);
  const [stats, setStats] = useState<VaultStats>({ documents: 0, chunks: 0, nodes: 0 });
  const [error, setError] = useState<string | null>(null);
  const scopeMenuRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const compactHoverUnlockTimerRef = useRef<number | null>(null);
  const fileMatchesCloseTimerRef = useRef<number | null>(null);
  const reachedTopWhileCompactRef = useRef(false);

  const modelSetupConfigured = useMemo(
    () => modelAvailability?.configured ?? false,
    [modelAvailability]
  );
  const modelSetupNotConfigured = useMemo(
    () => modelAvailability?.status_code === MODEL_NOT_CONFIGURED_CODE || !modelSetupConfigured,
    [modelAvailability?.status_code, modelSetupConfigured]
  );
  const modelSetupReady = useMemo(
    () =>
      Boolean(modelAvailability?.configured) &&
      Boolean(modelAvailability?.reachable) &&
      (modelAvailability?.missing_roles?.length ?? 1) === 0,
    [modelAvailability]
  );

  const {
    query,
    setQuery,
    answerResponse,
    loading,
    isSearching,
    searchElapsedMs,
    lastSearchDurationMs,
    runSearch
  } = useQueryFlow({
    aiLang,
    retrieveTopK,
    selectedScopePaths,
    modelSetupReady,
    onError: (message) => setError(message),
    toUiErrorMessage,
    onSearchStart: () => {
      setIsSearchBarCompact(false);
      setError(null);
      setExpandedSourceKeys(new Set());
      setFileMatchesOpen(false);
    }
  });

  const visibleEvidence = useMemo(
    () => answerResponse?.evidence.slice(0, retrieveTopK) ?? [],
    [answerResponse, retrieveTopK]
  );
  const visibleEvidenceGroups = useMemo<VisibleEvidenceGroup[]>(
    () => buildVisibleEvidenceGroups(visibleEvidence),
    [visibleEvidence]
  );
  const visibleCitations = useMemo<VisibleCitation[]>(
    () => buildVisibleCitations(answerResponse?.citations ?? [], retrieveTopK),
    [answerResponse, retrieveTopK]
  );
  const measuredMetricsTotalMs = useMemo(() => {
    if (!answerResponse) {
      return 0;
    }
    const metrics = answerResponse.metrics;
    return (
      metrics.query_analysis_ms +
      metrics.doc_recall_ms +
      metrics.doc_lexical_ms +
      metrics.doc_merge_ms +
      metrics.chunk_lexical_ms +
      metrics.chunk_dense_ms +
      metrics.merge_ms +
      metrics.answer_ms
    );
  }, [answerResponse]);
  const metricRows = useMemo<MetricRow[]>(() => {
    if (!answerResponse) {
      return [];
    }
    const metrics = answerResponse.metrics;
    return [
      { key: "answer_ms", label: t("metricAnswer"), value: metrics.answer_ms },
      { key: "doc_recall_ms", label: t("metricDocRecall"), value: metrics.doc_recall_ms },
      { key: "chunk_dense_ms", label: t("metricChunkDense"), value: metrics.chunk_dense_ms },
      { key: "chunk_lexical_ms", label: t("metricChunkLexical"), value: metrics.chunk_lexical_ms },
      { key: "query_analysis_ms", label: t("metricQueryAnalysis"), value: metrics.query_analysis_ms },
      { key: "doc_lexical_ms", label: t("metricDocLexical"), value: metrics.doc_lexical_ms },
      { key: "doc_merge_ms", label: t("metricDocMerge"), value: metrics.doc_merge_ms },
      { key: "merge_ms", label: t("metricMerge"), value: metrics.merge_ms }
    ]
      .sort((a, b) => b.value - a.value);
  }, [answerResponse, t]);
  const canSubmit = useMemo(
    () => query.trim().length > 0 && !loading && !modelSetupNotConfigured,
    [loading, modelSetupNotConfigured, query]
  );
  const showSearchDone = useMemo(
    () => isSearching && !loading && !error && answerResponse !== null,
    [answerResponse, error, isSearching, loading]
  );
  const selectedScopeLabel = useMemo(() => {
    if (selectedScopePaths.length === 0) {
      return t("scopeAll");
    }
    return t("scopeSelectedCount", { count: selectedScopePaths.length });
  }, [selectedScopePaths.length, t]);
  const headerWatchRoot = useMemo(() => watchRoot.trim() || "-", [watchRoot]);
  const headerSelectedCount = useMemo(
    () =>
      selectedScopePaths.length === 0
        ? t("scopeAll")
        : t("scopeSelectedCount", { count: selectedScopePaths.length }),
    [selectedScopePaths.length, t]
  );

  const selectedScopeSet = useMemo(() => new Set(selectedScopePaths), [selectedScopePaths]);
  const isSearchBarCollapsed =
    isSearching &&
    isSearchBarCompact &&
    !isSearchBarHovering &&
    !scopeMenuOpen;
  const searchPlaceholder = useMemo(
    () => (modelSetupNotConfigured ? t("modelNotConfiguredInline") : t("askPlaceholder")),
    [modelSetupNotConfigured, t]
  );
  const activeModelProfile = useMemo(
    () =>
      modelSettings.active_provider === "ollama_local"
        ? modelSettings.local_profile
        : modelSettings.remote_profile,
    [modelSettings]
  );
  const scopeViewItems = useMemo(() => {
    return scopeItems.map((item) => {
      const key = normalizeScopeKey(item.relative_path ?? "", item.path);
      const slashIndex = key.lastIndexOf("/");
      const parentKey = slashIndex >= 0 ? key.slice(0, slashIndex) : "";
      return { ...item, key, parentKey };
    });
  }, [scopeItems]);
  const scopeChildrenCountByParentKey = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of scopeViewItems) {
      const prev = counts.get(item.parentKey) ?? 0;
      counts.set(item.parentKey, prev + 1);
    }
    return counts;
  }, [scopeViewItems]);
  const visibleScopeItems = useMemo(() => {
    const byKey = new Map(scopeViewItems.map((item) => [item.key, item] as const));
    const visibilityMemo = new Map<string, boolean>();

    const isVisible = (item: (typeof scopeViewItems)[number]): boolean => {
      if (visibilityMemo.has(item.key)) {
        return visibilityMemo.get(item.key) ?? false;
      }
      if (!item.parentKey) {
        visibilityMemo.set(item.key, true);
        return true;
      }

      const parent = byKey.get(item.parentKey);
      if (!parent || !parent.is_dir) {
        visibilityMemo.set(item.key, true);
        return true;
      }

      const parentVisible = isVisible(parent);
      const visible = parentVisible && expandedScopeDirs.has(parent.path);
      visibilityMemo.set(item.key, visible);
      return visible;
    };

    return scopeViewItems.filter((item) => isVisible(item));
  }, [expandedScopeDirs, scopeViewItems]);

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(AI_LANG_STORAGE_KEY, aiLang);
    }
  }, [aiLang]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    window.localStorage.removeItem(LEGACY_THEME_MODE_STORAGE_KEY);
  }, [themeMode]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(FONT_PRESET_STORAGE_KEY, fontPreset);
  }, [fontPreset]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(FONT_SCALE_STORAGE_KEY, fontScale);
  }, [fontScale]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(RETRIEVE_TOP_K_STORAGE_KEY, String(retrieveTopK));
  }, [retrieveTopK]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const fontPresetMap: Record<
      FontPreset,
      { regular: string; mono: string }
    > = {
      system: {
        regular:
          '"PingFang SC","Microsoft YaHei","Segoe UI",Inter,"SF Pro Text","Noto Sans",sans-serif',
        mono:
          '"Cascadia Mono","JetBrains Mono","Maple Mono","IBM Plex Mono","Consolas","SFMono-Regular","Noto Sans Mono",monospace'
      },
      neo: {
        regular:
          '"HarmonyOS Sans SC","Segoe UI Variable","Segoe UI",Inter,"SF Pro Display","Noto Sans",sans-serif',
        mono:
          '"Berkeley Mono","JetBrains Mono","Cascadia Mono","IBM Plex Mono","Consolas","Noto Sans Mono",monospace'
      },
      mono: {
        regular:
          '"IBM Plex Sans","PingFang SC","Microsoft YaHei","Segoe UI",Inter,sans-serif',
        mono:
          '"IBM Plex Mono","Sarasa Mono SC","JetBrains Mono","Cascadia Mono","Consolas","Noto Sans Mono",monospace'
      }
    };
    const fontScaleMap: Record<FontScale, string> = {
      s: "14px",
      m: "16px",
      l: "18px"
    };

    root.style.setProperty("--app-font-family", fontPresetMap[fontPreset].regular);
    root.style.setProperty("--app-font-family-mono", fontPresetMap[fontPreset].mono);
    root.style.setProperty("--app-font-size", fontScaleMap[fontScale]);
    // Tailwind typography utilities are rem-based, so root(html) font-size must change.
    root.style.fontSize = fontScaleMap[fontScale];
    root.setAttribute("data-theme", themeMode);
    root.style.colorScheme = themeMode;
  }, [fontPreset, fontScale, themeMode]);

  useEffect(() => {
    let active = true;

    const loadStats = async () => {
      try {
        const raw = await getVaultStats();
        if (active) {
          setStats(normalizeStats(raw));
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadSettings = async () => {
      try {
        const settings = await getAppSettings();
        if (active) {
          setWatchRoot(settings.watch_root ?? "");
          setIndexingMode(normalizeIndexingMode(settings.indexing_mode));
          setResourceBudget(normalizeResourceBudget(settings.resource_budget));
          setScheduleStart(settings.schedule_start || "00:00");
          setScheduleEnd(settings.schedule_end || "06:00");
          if (!window.localStorage.getItem(AI_LANG_STORAGE_KEY) && settings.language) {
            const normalized = settings.language.toLowerCase();
            if (normalized.startsWith("zh")) {
              setAiLang("zh-CN");
            } else if (normalized.startsWith("en")) {
              setAiLang("en-US");
            }
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadIndexingStatus = async () => {
      try {
        const status = await getIndexingStatus();
        if (active) {
          setIndexingStatus({
            ...status,
            mode: normalizeIndexingMode(status.mode),
            resource_budget: normalizeResourceBudget(status.resource_budget)
          });
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadModelSettings = async () => {
      try {
        const settings = await getModelSettings();
        if (active) {
          setModelSettings(settings);
        }
        const profileConfigured =
          settings.active_provider === "ollama_local"
            ? settings.local_profile.endpoint.trim().length > 0 &&
              settings.local_profile.chat_model.trim().length > 0 &&
              settings.local_profile.graph_model.trim().length > 0 &&
              settings.local_profile.embed_model.trim().length > 0
            : settings.remote_profile.endpoint.trim().length > 0 &&
              (settings.remote_profile.api_key || "").trim().length > 0 &&
              settings.remote_profile.chat_model.trim().length > 0 &&
              settings.remote_profile.graph_model.trim().length > 0 &&
              settings.remote_profile.embed_model.trim().length > 0;
        if (!profileConfigured) {
          if (active) {
            setProviderModels({ from_folder: [], from_service: [], merged: [] });
          }
          return;
        }

        try {
          const profile =
            settings.active_provider === "ollama_local"
              ? settings.local_profile
              : settings.remote_profile;
          const models = await listProviderModels({
            provider: settings.active_provider,
            endpoint: profile.endpoint,
            apiKey:
              settings.active_provider === "openai_compatible"
                ? settings.remote_profile.api_key || null
                : null,
            modelsRoot:
              settings.active_provider === "ollama_local"
                ? settings.local_profile.models_root || null
                : null
          });
          if (active) {
            setProviderModels(models);
          }
        } catch {
          if (active) {
            setProviderModels({ from_folder: [], from_service: [], merged: [] });
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadEnterprisePolicy = async () => {
      try {
        const policy = await getEnterprisePolicy();
        if (active) {
          setEnterprisePolicy(policy);
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadModelAvailability = async () => {
      try {
        const availability = await validateModelSetup();
        if (active) {
          setModelAvailability(availability);
          if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
            setError(null);
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    void loadStats();
    void loadSettings();
    void loadIndexingStatus();
    void loadEnterprisePolicy();
    void loadModelSettings().then(() => {
      void loadModelAvailability();
    });
    const timer = window.setInterval(() => {
      void loadStats();
      void loadIndexingStatus();
    }, 5000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    let active = true;

    const loadScopes = async () => {
      if (!isTauriHostAvailable()) {
        return;
      }
      setScopeLoading(true);
      try {
        const scopes = await listSearchScopes();
        if (!active) {
          return;
        }
        setScopeItems(scopes);
        setSelectedScopePaths((prev) => prev.filter((path) => scopes.some((s) => s.path === path)));
        setExpandedScopeDirs(
          (prev) =>
            new Set(
              [...prev].filter((path) => scopes.some((s) => s.is_dir && s.path === path))
            )
        );
      } catch (err) {
        if (active) {
          setError(toUiErrorMessage(err));
        }
      } finally {
        if (active) {
          setScopeLoading(false);
        }
      }
    };

    void loadScopes();
    return () => {
      active = false;
    };
  }, [watchRoot]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!scopeMenuOpen) {
        return;
      }
      if (!scopeMenuRef.current) {
        return;
      }
      const target = event.target as Node | null;
      if (target && !scopeMenuRef.current.contains(target)) {
        setScopeMenuOpen(false);
      }
    };

    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [scopeMenuOpen]);

  useEffect(() => {
    let mounted = true;

    const syncMaximizeState = async () => {
      if (!isTauriHostAvailable()) {
        return;
      }

      try {
        const maximized = await getCurrentWindow().isMaximized();
        if (mounted) {
          setIsMaximized(maximized);
        }
      } catch {
        // Ignore; this is best-effort UI sync.
      }
    };

    void syncMaximizeState();

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    const onGlobalKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === ",") {
        event.preventDefault();
        setIsSettingsOpen((prev) => !prev);
        return;
      }

      if (event.key === "Escape" && isOnboardingOpen) {
        event.preventDefault();
        setIsOnboardingOpen(false);
        return;
      }

      if (event.key === "Escape" && isSettingsOpen) {
        event.preventDefault();
        setIsSettingsOpen(false);
      }
    };

    window.addEventListener("keydown", onGlobalKeyDown);
    return () => window.removeEventListener("keydown", onGlobalKeyDown);
  }, [isOnboardingOpen, isSettingsOpen]);

  useEffect(() => {
    let cancelled = false;
    const refreshOnProviderChange = async () => {
      const profileConfigured =
        modelSettings.active_provider === "ollama_local"
          ? modelSettings.local_profile.endpoint.trim().length > 0 &&
            modelSettings.local_profile.chat_model.trim().length > 0 &&
            modelSettings.local_profile.graph_model.trim().length > 0 &&
            modelSettings.local_profile.embed_model.trim().length > 0
          : modelSettings.remote_profile.endpoint.trim().length > 0 &&
            (modelSettings.remote_profile.api_key || "").trim().length > 0 &&
            modelSettings.remote_profile.chat_model.trim().length > 0 &&
            modelSettings.remote_profile.graph_model.trim().length > 0 &&
            modelSettings.remote_profile.embed_model.trim().length > 0;
      if (!profileConfigured) {
        setProviderModels({ from_folder: [], from_service: [], merged: [] });
        return;
      }
      try {
        const profile =
          modelSettings.active_provider === "ollama_local"
            ? modelSettings.local_profile
            : modelSettings.remote_profile;
        const models = await listProviderModels({
          provider: modelSettings.active_provider,
          endpoint: profile.endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "ollama_local"
              ? modelSettings.local_profile.models_root || null
              : null
        });
        if (!cancelled) {
          setProviderModels(models);
        }
      } catch {
        // keep previous candidates; explicit refresh button still available
      }
    };

    void refreshOnProviderChange();
    return () => {
      cancelled = true;
    };
  }, [modelSettings.active_provider]);

  useEffect(() => {
    if (!isSearching) {
      setIsSearchBarCompact(false);
      setIsSearchBarHovering(false);
      setIsSearchInputFocused(false);
      setAllowCompactHoverExpand(true);
    }
  }, [isSearching]);

  useEffect(() => {
    if (!isSearchBarCompact) {
      setIsSearchBarHovering(false);
      setAllowCompactHoverExpand(true);
    }
  }, [isSearchBarCompact]);

  useEffect(() => {
    return () => {
      if (compactHoverUnlockTimerRef.current !== null) {
        window.clearTimeout(compactHoverUnlockTimerRef.current);
      }
    };
  }, []);

  const refreshIndexingStatus = async () => {
    const status = await withTimeout(
      getIndexingStatus(),
      INDEXING_ACTION_TIMEOUT_MS,
      uiLang === "zh-CN" ? "获取索引状态超时，请稍后重试。" : "Fetching indexing status timed out."
    );
    setIndexingStatus({
      ...status,
      mode: normalizeIndexingMode(status.mode),
      resource_budget: normalizeResourceBudget(status.resource_budget)
    });
  };

  const onSaveIndexingConfig = async () => {
    setIndexingBusy(true);
    try {
      const saved = await withTimeout(
        saveIndexingModeRemote({
          indexing_mode: indexingMode,
          resource_budget: resourceBudget,
          schedule_start: indexingMode === "scheduled" ? scheduleStart : null,
          schedule_end: indexingMode === "scheduled" ? scheduleEnd : null
        }),
        INDEXING_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存索引配置超时，请重试。" : "Saving indexing config timed out."
      );
      setIndexingMode(normalizeIndexingMode(saved.indexing_mode));
      setResourceBudget(normalizeResourceBudget(saved.resource_budget));
      setScheduleStart(saved.schedule_start || "00:00");
      setScheduleEnd(saved.schedule_end || "06:00");
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onTriggerReindex = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        triggerReindex(),
        INDEXING_ACTION_TIMEOUT_MS * 2,
        uiLang === "zh-CN" ? "触发重建索引超时，请稍后重试。" : "Triggering reindex timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onPauseIndexing = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        pauseIndexing(),
        INDEXING_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "暂停索引超时，请稍后重试。" : "Pausing indexing timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onResumeIndexing = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        resumeIndexing(),
        INDEXING_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "恢复索引超时，请稍后重试。" : "Resuming indexing timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void runSearch();
    }
  };

  useEffect(() => {
    if (!isTauriHostAvailable()) {
      return;
    }

    if (!isSearchInputFocused || scopeMenuOpen) {
      setFileMatchesOpen(false);
      return;
    }

    const q = query.trim();
    if (q.length < 2 || isSearchBarCollapsed) {
      setFileMatches([]);
      setFileMatchesOpen(false);
      return;
    }

    let canceled = false;
      const timer = window.setTimeout(async () => {
        try {
          const matches = await searchFiles({
            query: q,
            limit: 20,
            // File suggestions should not be constrained by the current scope selection;
            // otherwise selecting one file makes it impossible to discover and multi-select others.
            scopePaths: undefined
          });
          if (canceled) return;
          setFileMatches(matches);
          setFileMatchesOpen(matches.length > 0);
        } catch {
        if (canceled) return;
        setFileMatches([]);
        setFileMatchesOpen(false);
      }
    }, 70);

    return () => {
      canceled = true;
      window.clearTimeout(timer);
    };
  }, [
    isSearchBarCollapsed,
    isSearchInputFocused,
    query,
    scopeMenuOpen,
    // Intentionally not depending on selectedScopePaths; suggestions remain global.
  ]);

  const onMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onToggleMaximize = async () => {
    try {
      const win = getCurrentWindow();
      await win.toggleMaximize();
      const maximized = await win.isMaximized();
      setIsMaximized(maximized);
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onToggleThemeMode = () => {
    setThemeMode((prev) => (prev === "dark" ? "light" : "dark"));
  };

  const onProbeModelProvider = async () => {
    setModelBusy(true);
    try {
      const availability = await withTimeout(
        probeModelProvider({
          provider: modelSettings.active_provider,
          endpoint: activeModelProfile.endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "ollama_local"
              ? modelSettings.local_profile.models_root || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN"
          ? "模型服务连接超时，请检查地址或网络。"
          : "Model provider request timed out. Please check endpoint/network."
      );
      setModelAvailability(availability);
      if (!availability.reachable) {
        const first = availability.errors?.[0];
        throw new Error(
          first ? `${first.code}: ${first.message}` : uiLang === "zh-CN" ? "连接失败" : "Connection failed"
        );
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onRefreshProviderModels = async () => {
    setModelBusy(true);
    try {
      const models = await withTimeout(
        listProviderModels({
          provider: modelSettings.active_provider,
          endpoint: activeModelProfile.endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "ollama_local"
              ? modelSettings.local_profile.models_root || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "刷新模型列表超时，请稍后重试。" : "Refreshing model list timed out."
      );
      setProviderModels(models);
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onSaveModelSettings = async () => {
    setModelBusy(true);
    try {
      const saved = await withTimeout(
        saveModelSettingsRemote(modelSettings),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存模型设置超时，请重试。" : "Saving model settings timed out."
      );
      setModelSettings(saved);
      const availability = await withTimeout(
        validateModelSetup(),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "模型校验超时，请重试。" : "Model validation timed out."
      );
      setModelAvailability(availability);
      if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
        setProviderModels({ from_folder: [], from_service: [], merged: [] });
        setError(null);
      } else {
        const refreshedModels = await withTimeout(
          listProviderModels({
            provider: saved.active_provider,
            endpoint:
              saved.active_provider === "ollama_local"
                ? saved.local_profile.endpoint
                : saved.remote_profile.endpoint,
            apiKey:
              saved.active_provider === "openai_compatible"
                ? saved.remote_profile.api_key || null
                : null,
            modelsRoot:
              saved.active_provider === "ollama_local" ? saved.local_profile.models_root || null : null
          }),
          MODEL_ACTION_TIMEOUT_MS,
          uiLang === "zh-CN" ? "刷新模型列表超时，请稍后重试。" : "Refreshing model list timed out."
        );
        setProviderModels(refreshedModels);
      }
      if (availability.reachable && (availability.missing_roles?.length ?? 0) === 0) {
        setIsOnboardingOpen(false);
        setError(null);
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onSaveEnterprisePolicy = async () => {
    setEnterpriseBusy(true);
    try {
      const saved = await withTimeout(
        saveEnterprisePolicyRemote(enterprisePolicy),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存企业策略超时，请重试。" : "Saving enterprise policy timed out."
      );
      setEnterprisePolicy(saved);
      try {
        const availability = await withTimeout(
          validateModelSetup(),
          MODEL_ACTION_TIMEOUT_MS,
          uiLang === "zh-CN" ? "模型校验超时，请重试。" : "Model validation timed out."
        );
        setModelAvailability(availability);
        if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
          setError(null);
        }
      } catch (err) {
        setModelAvailability(null);
        setError(toUiErrorMessage(err));
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setEnterpriseBusy(false);
    }
  };

  const onPullModel = async (model: string) => {
    setModelBusy(true);
    try {
      const availability = await withTimeout(
        pullModel({
          model,
          provider: modelSettings.active_provider,
          endpoint: activeModelProfile.endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS * 3,
        uiLang === "zh-CN" ? "拉取模型超时，请稍后重试。" : "Pull model timed out."
      );
      setModelAvailability(availability);
      const refreshedModels = await withTimeout(
        listProviderModels({
          provider: modelSettings.active_provider,
          endpoint: activeModelProfile.endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "ollama_local"
              ? modelSettings.local_profile.models_root || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "刷新模型列表超时，请稍后重试。" : "Refreshing model list timed out."
      );
      setProviderModels(refreshedModels);
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onSelectProvider = (provider: ModelProvider) => {
    setModelAvailability(null);
    setProviderModels({ from_folder: [], from_service: [], merged: [] });
    setError(null);
    setModelSettings((prev) => ({
      ...prev,
      active_provider: provider
    }));
  };

  const onPickLocalModelsRoot = async () => {
    if (!isTauriHostAvailable()) {
      throw new Error(TAURI_HOST_MISSING_MESSAGE);
    }
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: modelSettings.local_profile.models_root || undefined
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    const next = {
      ...modelSettings,
      local_profile: {
        ...modelSettings.local_profile,
        models_root: selected
      }
    };
    setModelSettings(next);
    if (next.active_provider === "ollama_local") {
      const models = await listProviderModels({
        provider: next.active_provider,
        endpoint: next.local_profile.endpoint,
        apiKey: null,
        modelsRoot: selected
      });
      setProviderModels(models);
    }
  };

  const onClearLocalModelsRoot = () => {
    setModelSettings((prev) => ({
      ...prev,
      local_profile: {
        ...prev.local_profile,
        models_root: ""
      }
    }));
    setProviderModels((prev) => ({ ...prev, from_folder: [], merged: prev.from_service }));
  };

  const updateActiveOnboardingProfile = (
    patch: Partial<{
      endpoint: string;
      api_key?: string | null;
      chat_model: string;
      graph_model: string;
      embed_model: string;
    }>
  ) => {
    setModelSettings((prev) => {
      if (prev.active_provider === "ollama_local") {
        return {
          ...prev,
          local_profile: { ...prev.local_profile, ...patch }
        };
      }
      return {
        ...prev,
        remote_profile: { ...prev.remote_profile, ...patch }
      };
    });
  };

  const toggleSourceExpanded = (key: string) => {
    setExpandedSourceKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const toggleCitationExpanded = (key: string) => {
    setExpandedCitationKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const onOpenSourceLocation = async (path: string) => {
    try {
      await openSourceLocation(path);
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onPickWatchRoot = async () => {
    if (!isTauriHostAvailable()) {
      setError(TAURI_HOST_MISSING_MESSAGE);
      return;
    }

    setIsPickingWatchRoot(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: watchRoot || undefined
      });

      if (!selected || Array.isArray(selected)) {
        return;
      }

      const settings = await saveWatchRootRemote(selected);
      setWatchRoot(settings.watch_root ?? selected);
      setSelectedScopePaths([]);
      setExpandedScopeDirs(new Set());

      const raw = await getVaultStats();
      setStats(normalizeStats(raw));
      setError(null);
    } catch (err) {
      setError(toUiErrorMessage(err));
    } finally {
      setIsPickingWatchRoot(false);
    }
  };

  const onToggleScopePath = (path: string) => {
    setSelectedScopePaths((prev) => {
      if (prev.includes(path)) {
        return prev.filter((p) => p !== path);
      }
      return [...prev, path];
    });
  };

  const onClearScopeSelection = () => {
    setSelectedScopePaths([]);
  };
  const onToggleScopeDirExpanded = (path: string) => {
    setExpandedScopeDirs((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const onResultScroll = (event: ReactUIEvent<HTMLElement>) => {
    const scrollTop = event.currentTarget.scrollTop;
    const shouldCompact = scrollTop > 2;
    if (shouldCompact) {
      reachedTopWhileCompactRef.current = false;
      if (allowCompactHoverExpand) {
        setAllowCompactHoverExpand(false);
      }
      if (compactHoverUnlockTimerRef.current !== null) {
        window.clearTimeout(compactHoverUnlockTimerRef.current);
      }
      compactHoverUnlockTimerRef.current = window.setTimeout(() => {
        setAllowCompactHoverExpand(true);
      }, 260);
      if (scopeMenuOpen) setScopeMenuOpen(false);
      if (isSearchBarHovering) setIsSearchBarHovering(false);
      if (isSearchInputFocused) {
        setIsSearchInputFocused(false);
        searchInputRef.current?.blur();
      }
      setIsSearchBarCompact((prev) => (prev === shouldCompact ? prev : shouldCompact));
      return;
    }
    reachedTopWhileCompactRef.current = true;
  };

  const onResultWheel = (event: ReactWheelEvent<HTMLElement>) => {
    if (!isSearchBarCompact) {
      return;
    }
    if (event.deltaY < 0 && event.currentTarget.scrollTop <= 2 && reachedTopWhileCompactRef.current) {
      reachedTopWhileCompactRef.current = false;
      setIsSearchBarCompact(false);
    }
  };

  return (
    <div className="h-screen w-screen bg-[var(--bg-canvas)] text-[var(--text-primary)]">
      <div className="relative flex h-full w-full flex-col overflow-hidden bg-[var(--bg-canvas)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.08)]">
        <TitleBar
          t={t}
          headerWatchRoot={headerWatchRoot}
          headerSelectedCount={headerSelectedCount}
          themeMode={themeMode}
          isMaximized={isMaximized}
          onToggleThemeMode={onToggleThemeMode}
          onToggleSettings={() => setIsSettingsOpen((prev) => !prev)}
          onMinimize={onMinimize}
          onToggleMaximize={onToggleMaximize}
          onClose={onClose}
        />

        <div className="relative z-10 flex-1 overflow-hidden">
          <div
            className={`pointer-events-none absolute inset-0 z-0 ${
              themeMode === "dark"
                ? "bg-[radial-gradient(104%_74%_at_50%_-10%,rgba(181,210,238,0.17),transparent_64%),radial-gradient(70%_52%_at_18%_34%,rgba(111,154,200,0.1),transparent_76%),radial-gradient(66%_50%_at_84%_36%,rgba(104,145,189,0.09),transparent_78%),linear-gradient(180deg,#121b28_0%,#111926_46%,#101722_100%)]"
                : "bg-[radial-gradient(120%_80%_at_50%_-10%,rgba(255,255,255,0.82),transparent_62%),radial-gradient(72%_54%_at_18%_30%,rgba(196,220,244,0.42),transparent_78%),radial-gradient(68%_52%_at_84%_34%,rgba(205,225,244,0.36),transparent_80%),linear-gradient(180deg,#f8fbff_0%,#f4f8fd_56%,#f2f6fb_100%)]"
            }`}
          />

          <main className="relative z-10 mx-auto h-full w-full max-w-5xl px-6 pb-4 md:px-10">
            <SearchStage
              isSearching={isSearching}
              isSearchBarCollapsed={isSearchBarCollapsed}
              isSearchBarCompact={isSearchBarCompact}
              allowCompactHoverExpand={allowCompactHoverExpand}
              isSearchInputFocused={isSearchInputFocused}
              scopeMenuOpen={scopeMenuOpen}
              scopeLoading={scopeLoading}
              scopeItems={scopeItems}
              visibleScopeItems={visibleScopeItems}
              selectedScopeSet={selectedScopeSet}
              selectedScopeLabel={selectedScopeLabel}
              scopeChildrenCountByParentKey={scopeChildrenCountByParentKey}
              expandedScopeDirs={expandedScopeDirs}
              fileMatchesOpen={fileMatchesOpen}
              fileMatches={fileMatches}
              watchRoot={watchRoot}
              showSearchDone={showSearchDone}
              loading={loading}
              modelSetupNotConfigured={modelSetupNotConfigured}
              query={query}
              uiLang={uiLang}
              searchPlaceholder={searchPlaceholder}
              t={t}
              searchInputRef={searchInputRef}
              scopeMenuRef={scopeMenuRef}
              fileMatchesCloseTimerRef={fileMatchesCloseTimerRef}
              setQuery={setQuery}
              setIsSearchBarHovering={setIsSearchBarHovering}
              setScopeMenuOpen={setScopeMenuOpen}
              setIsSearchInputFocused={setIsSearchInputFocused}
              setFileMatchesOpen={setFileMatchesOpen}
              setSelectedScopePaths={setSelectedScopePaths}
              onKeyDown={onKeyDown}
              onClearScopeSelection={onClearScopeSelection}
              onToggleScopePath={onToggleScopePath}
              onToggleScopeDirExpanded={onToggleScopeDirExpanded}
            />

            <ResultStage
              isSearching={isSearching}
              isSearchBarCollapsed={isSearchBarCollapsed}
              isSearchBarCompact={isSearchBarCompact}
              loading={loading}
              error={error}
              answerResponse={answerResponse}
              searchElapsedMs={searchElapsedMs}
              lastSearchDurationMs={lastSearchDurationMs}
              formatElapsed={formatElapsed}
              onResultScroll={onResultScroll}
              onResultWheel={onResultWheel}
              visibleCitations={visibleCitations}
              expandedCitationKeys={expandedCitationKeys}
              onToggleCitationExpanded={toggleCitationExpanded}
              visibleEvidenceGroups={visibleEvidenceGroups}
              expandedSourceKeys={expandedSourceKeys}
              onToggleSourceExpanded={toggleSourceExpanded}
              onOpenSourceLocation={onOpenSourceLocation}
              markdownRemarkPlugins={MARKDOWN_REMARK_PLUGINS}
              markdownRehypePlugins={MARKDOWN_REHYPE_PLUGINS}
              metricRows={metricRows}
              measuredMetricsTotalMs={measuredMetricsTotalMs}
              t={t}
            />
          </main>
        </div>

        <StatusFooter t={t} stats={stats} />

        <AnimatePresence>
          {isSettingsOpen && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="absolute inset-x-0 bottom-0 top-9 z-30 flex justify-end overflow-hidden bg-[var(--overlay)]"
              onClick={() => setIsSettingsOpen(false)}
            >
              <SettingsModal
                open={isSettingsOpen}
                onBack={() => setIsSettingsOpen(false)}
                uiLang={uiLang}
                aiLang={aiLang}
                onUiLangChange={setUiLang}
                onAiLangChange={setAiLang}
                watchRoot={watchRoot}
                isPickingWatchRoot={isPickingWatchRoot}
                onPickWatchRoot={() => void onPickWatchRoot()}
                retrieveTopK={retrieveTopK}
                onRetrieveTopKChange={setRetrieveTopK}
                fontPreset={fontPreset}
                onFontPresetChange={setFontPreset}
                fontScale={fontScale}
                onFontScaleChange={setFontScale}
                themeMode={themeMode}
                onThemeModeChange={setThemeMode}
                modelSettings={modelSettings}
                enterprisePolicy={enterprisePolicy}
                modelAvailability={modelAvailability}
                providerModels={providerModels}
                modelBusy={modelBusy}
                enterpriseBusy={enterpriseBusy}
                onModelSettingsChange={setModelSettings}
                onEnterprisePolicyChange={setEnterprisePolicy}
                onSaveModelSettings={onSaveModelSettings}
                onSaveEnterprisePolicy={onSaveEnterprisePolicy}
                onProbeModelProvider={onProbeModelProvider}
                onRefreshProviderModels={onRefreshProviderModels}
                onPullModel={onPullModel}
                onPickLocalModelsRoot={onPickLocalModelsRoot}
                onClearLocalModelsRoot={onClearLocalModelsRoot}
                indexingMode={indexingMode}
                resourceBudget={resourceBudget}
                scheduleStart={scheduleStart}
                scheduleEnd={scheduleEnd}
                indexingStatus={indexingStatus}
                indexingBusy={indexingBusy}
                onIndexingModeChange={setIndexingMode}
                onResourceBudgetChange={setResourceBudget}
                onScheduleStartChange={setScheduleStart}
                onScheduleEndChange={setScheduleEnd}
                onSaveIndexingConfig={onSaveIndexingConfig}
                onTriggerReindex={onTriggerReindex}
                onPauseIndexing={onPauseIndexing}
                onResumeIndexing={onResumeIndexing}
              />
            </motion.div>
          )}
        </AnimatePresence>

        <OnboardingOverlay
          open={isOnboardingOpen}
          t={t}
          onboardingStep={onboardingStep}
          onClose={() => setIsOnboardingOpen(false)}
          onStepBack={() => setOnboardingStep((prev) => Math.max(0, prev - 1))}
          onStepNext={() => setOnboardingStep((prev) => Math.min(3, prev + 1))}
          onFinish={() => {
            void onSaveModelSettings()
              .then(() => {
                setIsOnboardingOpen(false);
                setOnboardingStep(0);
              })
              .catch(() => {
                // keep wizard open and show global error
              });
          }}
          onSelectProvider={onSelectProvider}
          modelSettings={modelSettings}
          activeModelProfile={activeModelProfile}
          updateActiveOnboardingProfile={updateActiveOnboardingProfile}
          providerModels={providerModels}
          modelAvailability={modelAvailability}
          modelBusy={modelBusy}
          modelSetupReady={modelSetupReady}
          onProbeModelProvider={onProbeModelProvider}
          onRefreshProviderModels={onRefreshProviderModels}
          onPullModel={onPullModel}
        />
      </div>
    </div>
  );
}


