import {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatePresence, motion } from "framer-motion";
import {
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  FileText,
  FolderOpen,
  LoaderCircle,
  Moon,
  Minus,
  Sun,
  Search,
  Settings as SettingsIcon,
  Sparkles,
  Square,
  X
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  FontPreset,
  FontScale,
  ModelAvailabilityDto,
  ModelSettingsDto,
  ModelProvider,
  ProviderModelsDto,
  ThemeMode,
  SettingsModal
} from "./components/SettingsModal";
import { useI18n } from "./i18n";
import type { Language } from "./i18n";

type VaultStats = {
  documents: number;
  chunks: number;
  nodes: number;
};

type VaultStatsRaw = Partial<
  VaultStats & {
    document_count: number;
    chunk_count: number;
    graph_node_count: number;
  }
>;

type Source = {
  score: number;
  path: string;
  content: string;
};

type ParsedResponse = {
  synthesis: string;
  sources: Source[];
};

type AppSettingsDto = {
  watch_root: string;
  language?: string | null;
};

type SearchScopeItem = {
  path: string;
  name: string;
  relative_path: string;
  is_dir: boolean;
  depth: number;
};

const TAURI_HOST_MISSING_MESSAGE = "未检测到 Tauri 宿主环境，请使用 cargo tauri dev 启动";
const AI_LANG_STORAGE_KEY = "memori-ai-language";
const THEME_STORAGE_KEY = "memori-theme";
const LEGACY_THEME_MODE_STORAGE_KEY = "memori-theme-mode";
const FONT_PRESET_STORAGE_KEY = "memori-font-preset";
const FONT_SCALE_STORAGE_KEY = "memori-font-scale";
const RETRIEVE_TOP_K_STORAGE_KEY = "memori-retrieve-top-k";
const MODEL_ACTION_TIMEOUT_MS = 20000;

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
    return "m";
  }
  const saved = window.localStorage.getItem(FONT_SCALE_STORAGE_KEY);
  if (saved === "s" || saved === "l" || saved === "m") {
    return saved;
  }
  return "m";
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

function parseResponse(raw: string): ParsedResponse {
  const normalized = raw.replace(/\r\n/g, "\n");
  const parts = normalized.split("---");
  let synthesis = (parts.shift() ?? "").trim();
  let sourceSection = parts.join("---").replace(/参考来源[:：]/g, "").trim();

  // Fallback format: answer generation failed and backend directly returns
  // "本地大模型答案生成失败... + references" without the '---' separator.
  if (!sourceSection) {
    const firstRefIndex = normalized.search(/^\s*#\d+\s+相似度[:：]/m);
    if (firstRefIndex >= 0) {
      synthesis = normalized.slice(0, firstRefIndex).trim();
      sourceSection = normalized.slice(firstRefIndex).trim();
    }
  }

  if (!sourceSection) {
    return { synthesis, sources: [] };
  }

  const chunkBlocks = sourceSection
    .split(/(?=\n?#\d+\s+相似度[:：])/)
    .map((block) => block.trim())
    .filter((block) => block.length > 0);

  const sources: Source[] = [];

  for (const block of chunkBlocks) {
    const scoreMatch = block.match(/相似度[:：]\s*([0-9]*\.?[0-9]+)/);
    const pathMatch = block.match(/来源[:：]\s*(.+)/);
    const labeledContentMatch = block.match(/内容[:：]?\s*\n([\s\S]*)/);

    if (!scoreMatch || !pathMatch) {
      continue;
    }

    const score = Number.parseFloat(scoreMatch[1]);
    if (!Number.isFinite(score)) {
      continue;
    }

    const path = pathMatch[1].trim();
    let content = "";
    if (labeledContentMatch) {
      content = labeledContentMatch[1].trim();
    } else {
      // Current backend format has no explicit "内容:" label.
      // Strip metadata lines and keep the remaining text as snippet.
      content = block
        .split("\n")
        .filter((line) => {
          const trimmed = line.trim();
          if (!trimmed) return false;
          if (/^#\d+\s+相似度[:：]/.test(trimmed)) return false;
          if (/^来源[:：]/.test(trimmed)) return false;
          if (/^块序号[:：]/.test(trimmed)) return false;
          if (/^-{8,}$/.test(trimmed)) return false;
          return true;
        })
        .join("\n")
        .trim();
    }

    if (!path || !content) {
      continue;
    }

    sources.push({ score, path, content });
  }

  return { synthesis, sources };
}

function clampScore(score: number): number {
  if (!Number.isFinite(score)) {
    return 0;
  }
  return Math.max(0, Math.min(1, score));
}

function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path.trim());
}

function normalizeScopeKey(relativePath: string, fallback: string): string {
  const normalized = relativePath.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
  if (normalized) {
    return normalized;
  }
  return fallback.replaceAll("\\", "/");
}

function LiquidOrb({ score, semanticLabel }: { score: number; semanticLabel: string }) {
  const normalized = clampScore(score);
  const percentage = Math.round(normalized * 100);
  const liquidTop = `${100 - percentage}%`;

  return (
    <div className="flex items-center gap-3">
      <div className="glass-orb" style={{ "--liquid-top": liquidTop } as CSSProperties}>
        <span className="glass-orb-light" />
        <div className="glass-orb-fluid">
          <span className="glass-orb-liquid">
            <span className="glass-orb-foam" />
          </span>
          <span className="glass-orb-liquid glass-orb-liquid-back">
            <span className="glass-orb-foam" />
          </span>
        </div>
      </div>

      <div className="flex flex-col items-start gap-1 leading-none">
        <span className="text-sm font-mono font-bold text-[var(--accent)]">
          {percentage}%
        </span>
        <span className="text-[10px] tracking-[0.08em] text-[var(--text-secondary)]">{semanticLabel}</span>
      </div>
    </div>
  );
}

export default function App() {
  const { lang: uiLang, setLang: setUiLang, t } = useI18n();
  const [query, setQuery] = useState("");
  const [rawAnswer, setRawAnswer] = useState("");
  const [loading, setLoading] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
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
  const [selectedScopePaths, setSelectedScopePaths] = useState<string[]>([]);
  const [expandedScopeDirs, setExpandedScopeDirs] = useState<Set<string>>(() => new Set());
  const [expandedSourceKeys, setExpandedSourceKeys] = useState<Set<string>>(() => new Set());
  const [modelSettings, setModelSettings] = useState<ModelSettingsDto>(DEFAULT_MODEL_SETTINGS);
  const [modelAvailability, setModelAvailability] = useState<ModelAvailabilityDto | null>(null);
  const [providerModels, setProviderModels] = useState<ProviderModelsDto>({
    from_folder: [],
    from_service: [],
    merged: []
  });
  const [modelBusy, setModelBusy] = useState(false);
  const [stats, setStats] = useState<VaultStats>({ documents: 0, chunks: 0, nodes: 0 });
  const [error, setError] = useState<string | null>(null);
  const scopeMenuRef = useRef<HTMLDivElement | null>(null);

  const parsed = useMemo(() => parseResponse(rawAnswer), [rawAnswer]);
  const visibleSources = useMemo(
    () => parsed.sources.slice(0, retrieveTopK),
    [parsed.sources, retrieveTopK]
  );
  const canSubmit = useMemo(() => query.trim().length > 0 && !loading, [loading, query]);
  const showSearchDone = useMemo(
    () => isSearching && !loading && !error && rawAnswer.trim().length > 0,
    [error, isSearching, loading, rawAnswer]
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
  const modelSetupReady = useMemo(
    () => Boolean(modelAvailability?.reachable) && (modelAvailability?.missing_roles?.length ?? 1) === 0,
    [modelAvailability]
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

  useEffect(() => {
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
        const raw = await invoke<VaultStatsRaw>("get_vault_stats");
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
        const settings = await invoke<AppSettingsDto>("get_app_settings");
        if (active) {
          setWatchRoot(settings.watch_root ?? "");
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

    const loadModelSettings = async () => {
      try {
        const settings = await invoke<ModelSettingsDto>("get_model_settings");
        if (active) {
          setModelSettings(settings);
          const profile =
            settings.active_provider === "ollama_local"
              ? settings.local_profile
              : settings.remote_profile;
          const models = await invoke<ProviderModelsDto>("list_provider_models", {
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
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const validateModelSetup = async () => {
      try {
        const availability = await invoke<ModelAvailabilityDto>("validate_model_setup");
        if (active) {
          setModelAvailability(availability);
          if (!availability.reachable || (availability.missing_roles?.length ?? 0) > 0) {
            setIsOnboardingOpen(true);
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
          setIsOnboardingOpen(true);
        }
      }
    };

    void loadStats();
    void loadSettings();
    void loadModelSettings().then(() => {
      void validateModelSetup();
    });
    const timer = window.setInterval(() => {
      void loadStats();
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
        const scopes = await invoke<SearchScopeItem[]>("list_search_scopes");
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
      try {
        const profile =
          modelSettings.active_provider === "ollama_local"
            ? modelSettings.local_profile
            : modelSettings.remote_profile;
        const models = await invoke<ProviderModelsDto>("list_provider_models", {
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

  const runSearch = async () => {
    if (!canSubmit) {
      return;
    }
    if (!modelSetupReady) {
      setError(t("modelSetupNeeded"));
      setIsOnboardingOpen(true);
      return;
    }

    setIsSearching(true);
    setLoading(true);
    setError(null);
    setExpandedSourceKeys(new Set());

    try {
      const text = await invoke<string>("ask_vault", {
        query: query.trim(),
        lang: aiLang,
        topK: retrieveTopK,
        scopePaths: selectedScopePaths
      });
      setRawAnswer(text);
    } catch (error) {
      setRawAnswer("");
      setError(toUiErrorMessage(error));
    } finally {
      setLoading(false);
    }
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void runSearch();
    }
  };

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
        invoke<ModelAvailabilityDto>("probe_model_provider", {
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
        invoke<ProviderModelsDto>("list_provider_models", {
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
        invoke<ModelSettingsDto>("set_model_settings", {
          payload: modelSettings
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存模型设置超时，请重试。" : "Saving model settings timed out."
      );
      setModelSettings(saved);
      const availability = await withTimeout(
        invoke<ModelAvailabilityDto>("validate_model_setup"),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "模型校验超时，请重试。" : "Model validation timed out."
      );
      setModelAvailability(availability);
      const refreshedModels = await withTimeout(
        invoke<ProviderModelsDto>("list_provider_models", {
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
      if (availability.reachable && (availability.missing_roles?.length ?? 0) === 0) {
        setIsOnboardingOpen(false);
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onPullModel = async (model: string) => {
    setModelBusy(true);
    try {
      const availability = await withTimeout(
        invoke<ModelAvailabilityDto>("pull_model", {
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
        invoke<ProviderModelsDto>("list_provider_models", {
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
      const models = await invoke<ProviderModelsDto>("list_provider_models", {
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

  const onOpenSourceLocation = async (path: string) => {
    try {
      await invoke("open_source_location", { path });
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

      const settings = await invoke<AppSettingsDto>("set_watch_root", { path: selected });
      setWatchRoot(settings.watch_root ?? selected);
      setSelectedScopePaths([]);
      setExpandedScopeDirs(new Set());

      const raw = await invoke<VaultStatsRaw>("get_vault_stats");
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

  return (
    <div className="h-screen w-screen bg-[var(--bg-canvas)] text-[var(--text-primary)]">
      <div className="relative flex h-full w-full flex-col overflow-hidden bg-[var(--bg-canvas)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.08)]">
        <header
          data-tauri-drag-region=""
          className="relative z-50 flex h-9 shrink-0 items-center pl-2 pr-2 select-none bg-[var(--bg-canvas)]/95 backdrop-blur [app-region:drag] [-webkit-app-region:drag]"
        >
          <div
            data-tauri-drag-region=""
            className="pointer-events-none absolute inset-0 flex items-center justify-center px-44"
          >
            <div className="inline-flex min-w-0 max-w-[62vw] items-center gap-2 rounded-md border border-[var(--border-subtle)] bg-[var(--overlay)] px-3 py-1 text-[10px] text-[var(--text-secondary)]">
              <span className="shrink-0 uppercase tracking-[0.08em]">{t("watchRoot")}</span>
              <span className="min-w-0 truncate text-[var(--text-primary)]">{headerWatchRoot}</span>
              <span className="shrink-0 text-[var(--text-muted)]">|</span>
              <span className="shrink-0">{headerSelectedCount}</span>
            </div>
          </div>
          <div data-tauri-drag-region="" className="h-full flex-1 cursor-move" />
          <div className="flex items-center gap-1.5 [app-region:no-drag] [-webkit-app-region:no-drag]">
            <motion.button
              type="button"
              onClick={onToggleThemeMode}
              className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
              aria-label={t("themeToggle")}
              title={t("themeToggle")}
              whileTap={{ scale: 0.9 }}
              animate={{ rotate: themeMode === "dark" ? 0 : 180 }}
              transition={{ type: "spring", damping: 16, stiffness: 180 }}
            >
              {themeMode === "dark" ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
            </motion.button>
            <button
              type="button"
              onClick={() => setIsSettingsOpen((prev) => !prev)}
              className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
              aria-label={t("settings")}
              title={t("settings")}
            >
              <SettingsIcon className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={() => void onMinimize()}
              className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
              aria-label="Minimize"
              title="Minimize"
            >
              <Minus className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={() => void onToggleMaximize()}
              className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
              aria-label={isMaximized ? "Restore" : "Maximize"}
              title={isMaximized ? "Restore" : "Maximize"}
            >
              <Square className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={() => void onClose()}
              className="inline-flex items-center justify-center p-1 text-[var(--danger)] transition hover:text-red-400"
              aria-label="Close"
              title="Close"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </header>

        <div className="relative z-10 flex-1 overflow-hidden">
          <div className="pointer-events-none absolute inset-0 overflow-hidden">
            <div
              className="absolute left-1/2 top-[-220px] h-[620px] w-[620px] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(88,166,255,0.14),rgba(88,166,255,0)_72%)] transition-opacity duration-500"
              style={{ opacity: themeMode === "dark" ? 0.8 : 0.55 }}
            />
          </div>

          <main className="relative mx-auto h-full w-full max-w-5xl px-6 pb-4 md:px-10">
            <motion.div
              className="absolute left-6 right-6 md:left-10 md:right-10"
              animate={{
                top: isSearching ? "20px" : "45%",
                y: isSearching ? 0 : "-50%"
              }}
              transition={{ type: "spring", stiffness: 180, damping: 24 }}
            >
              <div className="relative mx-auto w-full max-w-4xl rounded-xl bg-[var(--bg-surface-1)] px-6 py-5 ring-1 ring-[var(--border-subtle)]">
                <div className="relative flex items-center gap-3">
                  <div ref={scopeMenuRef} className="relative shrink-0">
                    <button
                      type="button"
                      onClick={() => setScopeMenuOpen((prev) => !prev)}
                      className="inline-flex h-9 max-w-[170px] items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2.5 text-xs text-[var(--text-secondary)] transition hover:border-[var(--accent)]/60 hover:text-[var(--text-primary)]"
                      aria-label={t("scopeSelectTitle")}
                      title={t("scopeSelectTitle")}
                    >
                      <ChevronDown
                        className={`h-3.5 w-3.5 shrink-0 transition-transform ${
                          scopeMenuOpen ? "rotate-180" : ""
                        }`}
                      />
                      <span className="truncate">{selectedScopeLabel}</span>
                    </button>

                    <AnimatePresence>
                      {scopeMenuOpen && (
                        <motion.div
                          initial={{ opacity: 0, y: -6, scale: 0.98 }}
                          animate={{ opacity: 1, y: 0, scale: 1 }}
                          exit={{ opacity: 0, y: -4, scale: 0.98 }}
                          transition={{ duration: 0.16, ease: "easeOut" }}
                          className="absolute left-0 top-[calc(100%+10px)] z-40 w-[360px] rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-2 shadow-[0_20px_45px_rgba(0,0,0,0.5)] backdrop-blur"
                        >
                          <div className="mb-2 flex items-center justify-between px-1">
                            <span className="text-[11px] font-mono tracking-[0.08em] text-[var(--text-secondary)]">
                              {t("scopeSelectTitle")}
                            </span>
                            <button
                              type="button"
                              onClick={onClearScopeSelection}
                              className="text-[11px] text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                            >
                              {t("scopeAll")}
                            </button>
                          </div>

                          <div className="no-scrollbar max-h-72 overflow-y-auto pr-1">
                            {scopeLoading && (
                              <div className="px-2 py-3 text-xs text-[var(--text-secondary)]">{t("scopeLoading")}</div>
                            )}

                            {!scopeLoading && scopeItems.length === 0 && (
                              <div className="px-2 py-3 text-xs text-[var(--text-secondary)]">{t("scopeNoItems")}</div>
                            )}

                            {!scopeLoading &&
                              visibleScopeItems.map((item) => {
                                const selected = selectedScopeSet.has(item.path);
                                const displayName = item.name.trim() ? item.name : item.path;
                                const relativePath =
                                  item.relative_path.trim() || item.path;
                                const hasChildren =
                                  item.is_dir && (scopeChildrenCountByParentKey.get(item.key) ?? 0) > 0;
                                const isExpanded = expandedScopeDirs.has(item.path);

                                return (
                                  <div
                                    key={item.key}
                                    onClick={() => onToggleScopePath(item.path)}
                                    className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-left transition ${
                                      selected ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--bg-surface-2)]"
                                    }`}
                                    title={item.path}
                                    role="button"
                                    tabIndex={0}
                                    onKeyDown={(event) => {
                                      if (event.key === "Enter" || event.key === " ") {
                                        event.preventDefault();
                                        onToggleScopePath(item.path);
                                      }
                                    }}
                                  >
                                    <span
                                      className="flex min-w-0 items-center gap-2"
                                      style={{ paddingLeft: `${item.depth * 12}px` }}
                                    >
                                      {item.is_dir ? (
                                        <FolderOpen className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
                                      ) : (
                                        <FileText className="h-3.5 w-3.5 shrink-0 text-[var(--text-secondary)]" />
                                      )}
                                      <span className="min-w-0">
                                        <span className="block truncate text-xs text-[var(--text-primary)]">
                                          {displayName}
                                        </span>
                                        <span className="block truncate text-[10px] text-[var(--text-muted)]">
                                          {relativePath}
                                        </span>
                                      </span>
                                    </span>
                                    <span className="ml-2 inline-flex shrink-0 items-center gap-1">
                                      <span className="h-4 w-4">
                                        {selected ? (
                                          <Check className="h-4 w-4 text-[var(--accent)]" />
                                        ) : null}
                                      </span>
                                      {hasChildren ? (
                                        <button
                                          type="button"
                                          onClick={(event) => {
                                            event.stopPropagation();
                                            onToggleScopeDirExpanded(item.path);
                                          }}
                                          className="inline-flex h-4 w-4 items-center justify-center text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                          aria-label={isExpanded ? "Collapse folder" : "Expand folder"}
                                          title={isExpanded ? "Collapse folder" : "Expand folder"}
                                        >
                                          <ChevronRight
                                            className={`h-3.5 w-3.5 transition-transform ${
                                              isExpanded ? "rotate-90 text-[var(--accent)]" : ""
                                            }`}
                                          />
                                        </button>
                                      ) : (
                                        <span className="h-4 w-4" />
                                      )}
                                    </span>
                                  </div>
                                );
                              })}
                          </div>
                        </motion.div>
                      )}
                    </AnimatePresence>
                  </div>

                  <Search className="h-5 w-5 shrink-0 text-[var(--text-secondary)]" />
                  <input
                    type="text"
                    autoFocus
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={onKeyDown}
                    placeholder={t("askPlaceholder")}
                    className="w-full flex-1 border-none bg-transparent pr-10 text-2xl text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-0"
                  />
                </div>
                <AnimatePresence>
                  {isSearching && loading && (
                    <motion.div
                      initial={{ opacity: 0.2, scale: 0.95 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ repeat: Infinity, repeatType: "reverse", duration: 0.9 }}
                      className="absolute right-6 top-1/2 -translate-y-1/2 text-[var(--accent)]"
                    >
                      <LoaderCircle className="h-5 w-5 animate-spin" />
                    </motion.div>
                  )}
                  {showSearchDone && (
                    <motion.div
                      initial={{ opacity: 0, scale: 0.92 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: 0.2 }}
                      className="absolute right-6 top-1/2 -translate-y-1/2 text-[var(--accent)]"
                    >
                      <Check className="h-5 w-5" />
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            </motion.div>

            <AnimatePresence>
              {isSearching && (
                <motion.section
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 12 }}
                  transition={{ duration: 0.24, ease: "easeOut" }}
                  className="no-scrollbar mx-auto h-full w-full max-w-4xl overflow-y-auto pt-36"
                >
                  {loading && (
                    <div className="flex items-center gap-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-5 py-4 text-sm text-[var(--text-secondary)]">
                      <LoaderCircle className="h-4 w-4 animate-spin text-[var(--accent)]" />
                      {t("loading")}
                    </div>
                  )}

                  {!loading && error && (
                    <div className="rounded-xl border border-red-500/40 bg-red-500/10 px-5 py-4 text-sm text-red-300">
                      {error}
                    </div>
                  )}

                  {!loading && !error && parsed.synthesis && (
                    <article className="pb-8">
                      <div className="mb-6 border-l-2 border-[var(--accent)] pl-4">
                        <div className="mb-3 flex items-center gap-2">
                          <Sparkles className="h-4 w-4 text-[var(--accent)]" />
                          <span className="text-xs font-mono font-bold tracking-widest text-[var(--accent)]">
                            {t("synthesis")}
                          </span>
                        </div>
                        <p className="whitespace-pre-wrap break-words font-sans text-lg leading-relaxed text-[var(--text-primary)]">
                          {parsed.synthesis}
                        </p>
                      </div>

                      {visibleSources.length > 0 && (
                        <section className="mt-8">
                          <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
                            {t("contextSources")}
                          </div>

                          <div className="grid grid-cols-1 items-start gap-3 md:grid-cols-2">
                            {visibleSources.map((source, index) => {
                              const sourceKey = `${source.path}-${index}`;
                              const expanded = expandedSourceKeys.has(sourceKey);
                              const markdownPreview = expanded && isMarkdownFile(source.path);

                              return (
                                <div
                                  key={sourceKey}
                                  className="relative h-fit self-start flex flex-row items-start gap-4 rounded-xl border border-[var(--border-strong)] bg-[var(--bg-canvas)] p-4"
                                >
                                  <LiquidOrb score={source.score} semanticLabel={t("semanticRelevance")} />

                                  <div className="min-w-0 flex-1 pr-5">
                                    <div className="truncate font-mono text-xs text-[var(--text-secondary)]">
                                      {source.path}
                                    </div>
                                    {markdownPreview ? (
                                      <div className="md-preview mt-2 text-sm leading-6 text-[var(--text-secondary)]">
                                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                          {source.content}
                                        </ReactMarkdown>
                                      </div>
                                    ) : (
                                      <p
                                        className={
                                          expanded
                                            ? "mt-2 whitespace-pre-wrap break-words text-sm leading-6 text-[var(--text-muted)]"
                                            : "mt-2 overflow-hidden text-sm leading-6 text-[var(--text-muted)] [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2]"
                                        }
                                      >
                                        {source.content}
                                      </p>
                                    )}
                                  </div>

                                  <button
                                    type="button"
                                    onClick={() => void onOpenSourceLocation(source.path)}
                                    className="absolute top-3 right-8 p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                    aria-label={t("openSourceLocation")}
                                    title={t("openSourceLocation")}
                                  >
                                    <FolderOpen className="h-4 w-4" />
                                  </button>

                                  <button
                                    type="button"
                                    onClick={() => toggleSourceExpanded(sourceKey)}
                                    className="absolute top-3 right-3 p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                    aria-label={expanded ? t("collapseSource") : t("expandSource")}
                                    title={expanded ? t("collapseSource") : t("expandSource")}
                                  >
                                    {expanded ? (
                                      <ChevronUp className="h-4 w-4" />
                                    ) : (
                                      <ChevronDown className="h-4 w-4" />
                                    )}
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        </section>
                      )}
                    </article>
                  )}
                </motion.section>
              )}
            </AnimatePresence>
          </main>
        </div>

        <footer className="relative z-10 shrink-0 border-t border-[var(--border-subtle)] bg-[var(--bg-elevated)] backdrop-blur">
          <div className="mx-auto flex h-8 w-full max-w-5xl items-center justify-between px-6 text-[11px] text-[var(--text-secondary)] md:px-10">
            <span>
              {t("vaultStats", {
                docs: stats.documents,
                chunks: stats.chunks,
                nodes: stats.nodes
              })}
            </span>
            <span className="inline-flex items-center gap-2 text-emerald-400">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
              {t("localFirstDaemon")}
            </span>
          </div>
        </footer>

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
                modelAvailability={modelAvailability}
                providerModels={providerModels}
                modelBusy={modelBusy}
                onModelSettingsChange={setModelSettings}
                onSaveModelSettings={onSaveModelSettings}
                onProbeModelProvider={onProbeModelProvider}
                onRefreshProviderModels={onRefreshProviderModels}
                onPullModel={onPullModel}
                onPickLocalModelsRoot={onPickLocalModelsRoot}
                onClearLocalModelsRoot={onClearLocalModelsRoot}
              />
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence>
          {isOnboardingOpen && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="absolute inset-0 z-40 flex items-center justify-center bg-[var(--overlay)] backdrop-blur-sm"
            >
              <motion.div
                initial={{ opacity: 0, y: 12, scale: 0.98 }}
                animate={{ opacity: 1, y: 0, scale: 1 }}
                exit={{ opacity: 0, y: 10, scale: 0.98 }}
                transition={{ type: "spring", damping: 24, stiffness: 220 }}
                className="w-[680px] rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] p-5 shadow-2xl"
              >
                <div className="mb-4 flex items-center justify-between">
                  <div className="text-sm tracking-[0.14em] text-[var(--accent)] uppercase">
                    {t("setupWizard")}
                  </div>
                  <button
                    type="button"
                    onClick={() => setIsOnboardingOpen(false)}
                    className="text-xs text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
                  >
                    {t("closeWizard")}
                  </button>
                </div>

                <div className="mb-4 text-xs text-[var(--text-secondary)]">
                  Step {onboardingStep + 1}/4
                </div>

                {onboardingStep === 0 && (
                  <div className="space-y-3">
                    <div className="text-sm text-[var(--text-primary)]">{t("modelProvider")}</div>
                    <div className="flex gap-2">
                      <button
                        type="button"
                        onClick={() => onSelectProvider("ollama_local")}
                        className={`rounded-md border px-3 py-2 text-xs transition ${
                          modelSettings.active_provider === "ollama_local"
                            ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                            : "border-[var(--border-strong)] text-[var(--text-secondary)]"
                        }`}
                      >
                        {t("providerOllama")}
                      </button>
                      <button
                        type="button"
                        onClick={() => onSelectProvider("openai_compatible")}
                        className={`rounded-md border px-3 py-2 text-xs transition ${
                          modelSettings.active_provider === "openai_compatible"
                            ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                            : "border-[var(--border-strong)] text-[var(--text-secondary)]"
                        }`}
                      >
                        {t("providerOpenAI")}
                      </button>
                    </div>
                  </div>
                )}

                {onboardingStep === 1 && (
                  <div className="space-y-3">
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("modelEndpoint")}</div>
                      <input
                        value={activeModelProfile.endpoint}
                        onChange={(e) => updateActiveOnboardingProfile({ endpoint: e.target.value })}
                        className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                      />
                    </div>
                    {modelSettings.active_provider === "openai_compatible" && (
                      <div>
                        <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("modelApiKey")}</div>
                        <input
                          value={modelSettings.remote_profile.api_key ?? ""}
                          onChange={(e) => updateActiveOnboardingProfile({ api_key: e.target.value })}
                          className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                        />
                      </div>
                    )}
                  </div>
                )}

                {onboardingStep === 2 && (
                  <div className="space-y-3">
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("chatModel")}</div>
                      <select
                        value={activeModelProfile.chat_model}
                        onChange={(e) => updateActiveOnboardingProfile({ chat_model: e.target.value })}
                        className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                      >
                        {providerModels.merged.length === 0 ? (
                          <option value={activeModelProfile.chat_model}>{activeModelProfile.chat_model}</option>
                        ) : null}
                        {!providerModels.merged.includes(activeModelProfile.chat_model) ? (
                          <option value={activeModelProfile.chat_model}>{activeModelProfile.chat_model}</option>
                        ) : null}
                        {providerModels.merged.map((item) => (
                          <option key={`onboard-chat-${item}`} value={item}>
                            {item}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("graphModel")}</div>
                      <select
                        value={activeModelProfile.graph_model}
                        onChange={(e) => updateActiveOnboardingProfile({ graph_model: e.target.value })}
                        className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                      >
                        {providerModels.merged.length === 0 ? (
                          <option value={activeModelProfile.graph_model}>
                            {activeModelProfile.graph_model}
                          </option>
                        ) : null}
                        {!providerModels.merged.includes(activeModelProfile.graph_model) ? (
                          <option value={activeModelProfile.graph_model}>
                            {activeModelProfile.graph_model}
                          </option>
                        ) : null}
                        {providerModels.merged.map((item) => (
                          <option key={`onboard-graph-${item}`} value={item}>
                            {item}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("embedModel")}</div>
                      <select
                        value={activeModelProfile.embed_model}
                        onChange={(e) => updateActiveOnboardingProfile({ embed_model: e.target.value })}
                        className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                      >
                        {providerModels.merged.length === 0 ? (
                          <option value={activeModelProfile.embed_model}>
                            {activeModelProfile.embed_model}
                          </option>
                        ) : null}
                        {!providerModels.merged.includes(activeModelProfile.embed_model) ? (
                          <option value={activeModelProfile.embed_model}>
                            {activeModelProfile.embed_model}
                          </option>
                        ) : null}
                        {providerModels.merged.map((item) => (
                          <option key={`onboard-embed-${item}`} value={item}>
                            {item}
                          </option>
                        ))}
                      </select>
                    </div>
                  </div>
                )}

                {onboardingStep === 3 && (
                  <div className="space-y-3">
                    <div className="text-sm text-[var(--text-primary)]">{t("testConnection")}</div>
                    <div className="flex gap-2">
                      <button
                        type="button"
                        onClick={() => void onProbeModelProvider()}
                        disabled={modelBusy}
                        className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                      >
                        {t("testConnection")}
                      </button>
                      <button
                        type="button"
                        onClick={() => void onRefreshProviderModels()}
                        disabled={modelBusy}
                        className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                      >
                        {t("refreshModels")}
                      </button>
                      {modelSettings.active_provider === "ollama_local" &&
                        modelAvailability?.missing_roles?.includes("embed") && (
                          <button
                            type="button"
                            onClick={() => void onPullModel(activeModelProfile.embed_model)}
                            disabled={modelBusy}
                            className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                          >
                            {t("pullMissingModels")}
                          </button>
                        )}
                    </div>
                    <div className="rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                      {modelAvailability?.reachable ? t("modelStatusReachable") : t("modelStatusUnreachable")}
                      {" | "}
                      {modelAvailability?.missing_roles?.length
                        ? t("modelStatusMissing", {
                            roles: modelAvailability.missing_roles.join(", ")
                          })
                        : t("modelStatusReady")}
                    </div>
                  </div>
                )}

                <div className="mt-5 flex items-center justify-between">
                  <button
                    type="button"
                    onClick={() => setOnboardingStep((prev) => Math.max(0, prev - 1))}
                    disabled={onboardingStep === 0}
                    className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-primary)] disabled:opacity-50"
                  >
                    {t("previousStep")}
                  </button>
                  {onboardingStep < 3 ? (
                    <button
                      type="button"
                      onClick={() => setOnboardingStep((prev) => Math.min(3, prev + 1))}
                      className="rounded-md border border-[var(--accent)] bg-[var(--accent-soft)] px-3 py-1.5 text-xs text-[var(--accent)]"
                    >
                      {t("nextStep")}
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => {
                        void onSaveModelSettings()
                          .then(() => {
                            setIsOnboardingOpen(false);
                            setOnboardingStep(0);
                          })
                          .catch(() => {
                            // keep wizard open and show global error
                          });
                      }}
                      disabled={!modelSetupReady || modelBusy}
                      className="rounded-md border border-[var(--accent)] bg-[var(--accent-soft)] px-3 py-1.5 text-xs text-[var(--accent)]"
                    >
                      {t("finishSetup")}
                    </button>
                  )}
                </div>
              </motion.div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
