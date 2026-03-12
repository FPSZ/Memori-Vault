import {
  KeyboardEvent as ReactKeyboardEvent,
  UIEvent as ReactUIEvent,
  WheelEvent as ReactWheelEvent,
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
  Atom,
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
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import {
  FontPreset,
  FontScale,
  EnterprisePolicyDto,
  IndexingMode,
  IndexingStatusDto,
  ModelAvailabilityDto,
  ModelSettingsDto,
  ModelProvider,
  ProviderModelsDto,
  ResourceBudget,
  ThemeMode,
  SettingsModal
} from "./components/SettingsModal";
import { fadeSlideUpVariants } from "./components/MotionKit";
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

type AskStatus = "answered" | "insufficient_evidence" | "model_failed_with_evidence";

type CitationItem = {
  index: number;
  file_path: string;
  relative_path: string;
  chunk_index: number;
  heading_path: string[];
  excerpt: string;
};

type VisibleCitation = CitationItem & {
  citation_key: string;
  duplicate_count: number;
  is_long_excerpt: boolean;
};

type VisibleEvidenceGroup = {
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

type MetricRow = {
  key: string;
  label: string;
  value: number;
};

type EvidenceItem = {
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

type RetrievalMetrics = {
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
  query_flags: string[];
};

type AskResponseStructured = {
  status: AskStatus;
  answer: string;
  question: string;
  scope_paths: string[];
  citations: CitationItem[];
  evidence: EvidenceItem[];
  metrics: RetrievalMetrics;
};

type AppSettingsDto = {
  watch_root: string;
  language?: string | null;
  indexing_mode?: string | null;
  resource_budget?: string | null;
  schedule_start?: string | null;
  schedule_end?: string | null;
};

type SearchScopeItem = {
  path: string;
  name: string;
  relative_path: string;
  is_dir: boolean;
  depth: number;
};

type FileMatch = {
  file_path: string;
  file_name: string;
  parent_dir: string;
  ext: string;
  mtime_secs: number;
  file_size: number;
};

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

function formatElapsed(ms: number): string {
  const safe = Math.max(0, ms);
  if (safe < 60_000) {
    return `${(safe / 1000).toFixed(1)}s`;
  }
  const totalSeconds = Math.round(safe / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path.trim());
}

function buildCollapsedMarkdownPreview(content: string, maxLines = 8): string {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return normalized;
  }

  const fenceMatch = normalized.match(/```[\w-]*\n[\s\S]*?\n```/);
  if (fenceMatch) {
    const fenceIndex = fenceMatch.index ?? 0;
    const before = normalized.slice(0, fenceIndex).trim();
    const beforeLines = before ? before.split("\n").slice(-Math.min(maxLines, 6)).join("\n") : "";
    return beforeLines ? `${beforeLines}\n\n${fenceMatch[0]}` : fenceMatch[0];
  }

  return normalized.split("\n").slice(0, maxLines).join("\n");
}

function normalizeEvidenceContent(content: string): string {
  return content
    .replace(/\r\n/g, "\n")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function mergeEvidenceFragments(items: EvidenceItem[]): string {
  const uniqueFragments: string[] = [];

  for (const item of items) {
    const normalized = normalizeEvidenceContent(item.content);
    if (!normalized) {
      continue;
    }

    const duplicateIndex = uniqueFragments.findIndex((existing) => {
      if (existing === normalized) {
        return true;
      }
      if (existing.includes(normalized)) {
        return true;
      }
      if (normalized.includes(existing)) {
        return true;
      }
      return false;
    });

    if (duplicateIndex >= 0) {
      if (normalized.length > uniqueFragments[duplicateIndex].length) {
        uniqueFragments[duplicateIndex] = normalized;
      }
      continue;
    }

    uniqueFragments.push(normalized);
  }

  return uniqueFragments.join("\n\n---\n\n");
}

function formatMetricDuration(ms: number): string {
  return `${Math.round(Math.max(0, ms))}ms`;
}

function formatQueryFlag(flag: string, t: ReturnType<typeof useI18n>["t"]): string {
  switch (flag) {
    case "cjk":
      return t("flagCjk");
    case "ascii_identifier":
      return t("flagAsciiIdentifier");
    case "path_like":
      return t("flagPathLike");
    case "lookup_like":
      return t("flagLookupLike");
    default:
      break;
  }

  if (flag.startsWith("token_count:")) {
    return t("flagTokenCount", { count: flag.slice("token_count:".length) });
  }
  if (flag.startsWith("identifier_terms:")) {
    return t("flagIdentifierTerms", { count: flag.slice("identifier_terms:".length) });
  }
  if (flag.startsWith("filename_terms:")) {
    return t("flagFilenameTerms", { count: flag.slice("filename_terms:".length) });
  }
  if (flag.startsWith("query_family:")) {
    const family = flag.slice("query_family:".length);
    const value =
      family === "docs_explanatory"
        ? t("queryFamilyDocsExplanatory")
        : family === "docs_api_lookup"
          ? t("queryFamilyDocsApiLookup")
          : family === "implementation_lookup"
            ? t("queryFamilyImplementationLookup")
            : family;
    return t("flagQueryFamily", { value });
  }
  if (flag.startsWith("intent:")) {
    const intent = flag.slice("intent:".length);
    const value =
      intent === "repo_lookup"
        ? t("intentRepoLookup")
        : intent === "repo_question"
          ? t("intentRepoQuestion")
          : intent === "external_fact"
            ? t("intentExternalFact")
            : intent === "secret_request"
              ? t("intentSecretRequest")
              : intent === "missing_file_lookup"
                ? t("intentMissingFileLookup")
                : intent;
    return t("flagIntent", { value });
  }

  return flag;
}

function isLongCitationExcerpt(content: string): boolean {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return false;
  }
  return normalized.length > 420 || normalized.split("\n").length > 10;
}

function formatEvidenceReason(reason: EvidenceItem["reason"], t: ReturnType<typeof useI18n>["t"]): string {
  switch (reason) {
    case "both":
      return t("evidenceReasonBoth");
    case "lexical":
      return t("evidenceReasonLexical");
    case "dense":
      return t("evidenceReasonDense");
    default:
      return reason;
  }
}

function formatDocumentReason(
  reason: EvidenceItem["document_reason"],
  t: ReturnType<typeof useI18n>["t"]
): string {
  switch (reason) {
    case "both":
      return t("documentReasonBoth");
    case "lexical":
      return t("documentReasonLexical");
    case "lexical_strict":
      return t("documentReasonLexicalStrict");
    case "lexical_broad":
      return t("documentReasonLexicalBroad");
    case "mixed":
      return t("documentReasonMixed");
    case "exact_path":
      return t("documentReasonExactPath");
    case "exact_symbol":
      return t("documentReasonExactSymbol");
    case "docs_phrase":
      return t("documentReasonDocsPhrase");
    case "filename":
      return t("documentReasonFilename");
    case "scope":
      return t("documentReasonScope");
    default:
      return reason;
  }
}

function statusToneClasses(status: AskStatus): string {
  switch (status) {
    case "answered":
      return "border-[color-mix(in_srgb,var(--accent)_34%,transparent)] bg-[color-mix(in_srgb,var(--accent)_12%,var(--bg-surface-1)_88%)] text-[color-mix(in_srgb,var(--accent)_76%,var(--text-primary)_24%)]";
    case "model_failed_with_evidence":
      return "border-amber-500/30 bg-amber-500/10 text-amber-200";
    default:
      return "border-[var(--border-strong)] bg-[var(--bg-surface-2)] text-[var(--text-secondary)]";
  }
}

function normalizeScopeKey(relativePath: string, fallback: string): string {
  const normalized = relativePath.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
  if (normalized) {
    return normalized;
  }
  return fallback.replaceAll("\\", "/");
}

export default function App() {
  const { lang: uiLang, setLang: setUiLang, t } = useI18n();
  const [query, setQuery] = useState("");
  const [answerResponse, setAnswerResponse] = useState<AskResponseStructured | null>(null);
  const [loading, setLoading] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [isSearchBarCompact, setIsSearchBarCompact] = useState(false);
  const [isSearchBarHovering, setIsSearchBarHovering] = useState(false);
  const [isSearchInputFocused, setIsSearchInputFocused] = useState(false);
  const [allowCompactHoverExpand, setAllowCompactHoverExpand] = useState(true);
  const [searchElapsedMs, setSearchElapsedMs] = useState(0);
  const [lastSearchDurationMs, setLastSearchDurationMs] = useState<number | null>(null);
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
  const searchStartedAtRef = useRef<number | null>(null);
  const compactHoverUnlockTimerRef = useRef<number | null>(null);
  const fileMatchesCloseTimerRef = useRef<number | null>(null);
  const reachedTopWhileCompactRef = useRef(false);

  const visibleEvidence = useMemo(
    () => answerResponse?.evidence.slice(0, retrieveTopK) ?? [],
    [answerResponse, retrieveTopK]
  );
  const visibleEvidenceGroups = useMemo<VisibleEvidenceGroup[]>(() => {
    const groups = new Map<string, EvidenceItem[]>();
    for (const source of visibleEvidence) {
      const key = source.file_path.toLowerCase();
      const bucket = groups.get(key);
      if (bucket) {
        bucket.push(source);
      } else {
        groups.set(key, [source]);
      }
    }

    return Array.from(groups.values())
      .map((items) => {
        const sortedItems = [...items].sort((a, b) => a.chunk_rank - b.chunk_rank);
        const first = sortedItems[0];
        const headingPaths = Array.from(
          new Set(
            sortedItems
              .map((item) => item.heading_path.join(" > ").trim())
              .filter((value) => value.length > 0)
          )
        );
        const blockKinds = Array.from(
          new Set(sortedItems.map((item) => item.block_kind.trim()).filter(Boolean))
        );
        const documentReasons = Array.from(
          new Set(sortedItems.map((item) => item.document_reason))
        );
        const reasons = Array.from(new Set(sortedItems.map((item) => item.reason)));
        const chunkRanks = sortedItems.map((item) => item.chunk_rank);
        return {
          evidence_key: `${first.file_path.toLowerCase()}::${chunkRanks.join(",")}`,
          file_path: first.file_path,
          relative_path: first.relative_path,
          heading_paths: headingPaths,
          block_kinds: blockKinds,
          document_reasons: documentReasons,
          reasons,
          document_rank: Math.min(...sortedItems.map((item) => item.document_rank)),
          top_chunk_rank: Math.min(...chunkRanks),
          chunk_ranks: chunkRanks,
          content: mergeEvidenceFragments(sortedItems),
          fragment_count: sortedItems.length
        };
      })
      .sort((a, b) => {
        if (a.document_rank !== b.document_rank) {
          return a.document_rank - b.document_rank;
        }
        return a.top_chunk_rank - b.top_chunk_rank;
      });
  }, [visibleEvidence]);
  const visibleCitations = useMemo<VisibleCitation[]>(() => {
    const grouped = new Map<string, VisibleCitation>();
    for (const citation of answerResponse?.citations ?? []) {
      const excerpt = citation.excerpt.trim();
      const citationKey = `${citation.file_path.toLowerCase()}::${excerpt}`;
      const existing = grouped.get(citationKey);
      if (existing) {
        existing.duplicate_count += 1;
        continue;
      }
      grouped.set(citationKey, {
        ...citation,
        citation_key: citationKey,
        duplicate_count: 1,
        is_long_excerpt: isLongCitationExcerpt(citation.excerpt)
      });
    }

    return Array.from(grouped.values())
      .map((citation, index) => ({
        ...citation,
        index: index + 1
      }))
      .slice(0, retrieveTopK);
  }, [answerResponse, retrieveTopK]);
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
  const modelSetupConfigured = useMemo(
    () => modelAvailability?.configured ?? false,
    [modelAvailability]
  );
  const modelSetupNotConfigured = useMemo(
    () => modelAvailability?.status_code === MODEL_NOT_CONFIGURED_CODE || !modelSetupConfigured,
    [modelAvailability?.status_code, modelSetupConfigured]
  );
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
  const modelSetupReady = useMemo(
    () =>
      Boolean(modelAvailability?.configured) &&
      Boolean(modelAvailability?.reachable) &&
      (modelAvailability?.missing_roles?.length ?? 1) === 0,
    [modelAvailability]
  );
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
        const status = await invoke<IndexingStatusDto>("get_indexing_status");
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
        const settings = await invoke<ModelSettingsDto>("get_model_settings");
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
        const policy = await invoke<EnterprisePolicyDto>("get_enterprise_policy");
        if (active) {
          setEnterprisePolicy(policy);
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
      void validateModelSetup();
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

  useEffect(() => {
    if (!loading) {
      return;
    }
    const updateElapsed = () => {
      const startedAt = searchStartedAtRef.current;
      if (startedAt == null) {
        return;
      }
      setSearchElapsedMs(performance.now() - startedAt);
    };
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 100);
    return () => window.clearInterval(timer);
  }, [loading]);

  const refreshIndexingStatus = async () => {
    const status = await withTimeout(
      invoke<IndexingStatusDto>("get_indexing_status"),
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
        invoke<AppSettingsDto>("set_indexing_mode", {
          payload: {
            indexing_mode: indexingMode,
            resource_budget: resourceBudget,
            schedule_start: indexingMode === "scheduled" ? scheduleStart : null,
            schedule_end: indexingMode === "scheduled" ? scheduleEnd : null
          }
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
        invoke<string>("trigger_reindex"),
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
        invoke("pause_indexing"),
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
        invoke("resume_indexing"),
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

  const runSearch = async (overrideScopePaths?: string[]) => {
    if (!canSubmit) {
      return;
    }
    if (!modelSetupReady) {
      return;
    }

    setIsSearching(true);
    setIsSearchBarCompact(false);
    setLoading(true);
    setSearchElapsedMs(0);
    setLastSearchDurationMs(null);
    searchStartedAtRef.current = performance.now();
    setError(null);
    setAnswerResponse(null);
    setExpandedSourceKeys(new Set());
    setFileMatchesOpen(false);

    try {
      const scopePaths = overrideScopePaths ?? selectedScopePaths;
      const response = await invoke<AskResponseStructured>("ask_vault_structured", {
        query: query.trim(),
        lang: aiLang,
        topK: retrieveTopK,
        scopePaths
      });
      setAnswerResponse(response);
    } catch (error) {
      setAnswerResponse(null);
      setError(toUiErrorMessage(error));
    } finally {
      const startedAt = searchStartedAtRef.current;
      if (startedAt != null) {
        const elapsed = performance.now() - startedAt;
        setSearchElapsedMs(elapsed);
        setLastSearchDurationMs(elapsed);
      }
      searchStartedAtRef.current = null;
      setLoading(false);
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
          const matches = await invoke<FileMatch[]>("search_files", {
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
      if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
        setProviderModels({ from_folder: [], from_service: [], merged: [] });
        setError(null);
      } else {
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
        invoke<EnterprisePolicyDto>("set_enterprise_policy", {
          payload: enterprisePolicy
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存企业策略超时，请重试。" : "Saving enterprise policy timed out."
      );
      setEnterprisePolicy(saved);
      try {
        const availability = await withTimeout(
          invoke<ModelAvailabilityDto>("validate_model_setup"),
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
        <header
          data-tauri-drag-region=""
          className="relative z-50 flex h-9 shrink-0 items-center pl-2 pr-2 select-none bg-[var(--bg-canvas)]/95 backdrop-blur [app-region:drag] [-webkit-app-region:drag]"
        >
          <div
            data-tauri-drag-region=""
            className="pointer-events-none absolute inset-0 flex items-center justify-center px-44"
          >
            <div className="inline-flex min-w-0 max-w-[62vw] items-center gap-2 px-1 text-[10px] text-[var(--text-secondary)]">
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
              className="absolute left-6 right-6 z-40 md:left-10 md:right-10"
              animate={{
                top: isSearching ? (isSearchBarCollapsed ? "6px" : isSearchBarCompact ? "8px" : "20px") : "45%",
                y: isSearching ? 0 : "-50%"
              }}
              transition={{ duration: 0.18, ease: "easeOut" }}
            >
              <motion.div
                onMouseEnter={() => {
                  if (isSearchBarCompact && allowCompactHoverExpand) {
                    setIsSearchBarHovering(true);
                  }
                }}
                onMouseLeave={() => {
                  if (isSearchBarCompact && !isSearchInputFocused && !scopeMenuOpen) {
                    setIsSearchBarHovering(false);
                  }
                }}
                className={`relative mx-auto w-full transition-[max-width,opacity,box-shadow,background-color,border-color] duration-300 ease-out will-change-[max-width,opacity] focus-within:ring-1 focus-within:ring-[var(--line-soft)] focus-within:shadow-[var(--float-shadow-focus)] ${
                  isSearchBarCollapsed
                    ? "max-w-[300px] rounded-full overflow-hidden border-0 bg-transparent px-0 py-0 shadow-none ring-0 focus-within:ring-0"
                    : isSearching && isSearchBarCompact
                    ? "max-w-3xl rounded-full px-4 py-2.5"
                    : "max-w-4xl rounded-xl px-6 py-5"
                } ${
                  isSearchBarCollapsed
                    ? ""
                    : isSearching && isSearchBarCompact
                    ? "bg-[var(--bg-surface-1)] ring-0 shadow-[0_2px_10px_rgba(15,23,42,0.08)]"
                    : isSearching
                    ? "bg-[var(--bg-surface-1)] ring-0 shadow-[var(--float-shadow)]"
                    : "bg-[var(--bg-surface-1)] ring-0 shadow-[var(--float-shadow)]"
                }`}
              >
                {isSearchBarCollapsed ? (
                  <button
                    type="button"
                    onClick={() => {
                      setIsSearchBarHovering(true);
                      requestAnimationFrame(() => searchInputRef.current?.focus());
                    }}
                    aria-label={searchPlaceholder}
                    className="block h-1.5 w-full appearance-none rounded-full border-0 bg-[var(--search-collapsed-bar)] p-0 shadow-[0_2px_8px_rgba(15,23,42,0.12)] outline-none focus:border-0 focus:outline-none focus-visible:outline-none focus:ring-0 focus-visible:ring-0"
                  />
                ) : (
                  <>
                    <div className="relative flex items-center gap-3">
                  <div ref={scopeMenuRef} className="relative shrink-0">
                    <button
                      type="button"
                      onClick={() => setScopeMenuOpen((prev) => !prev)}
                      className={`inline-flex max-w-[170px] items-center gap-1.5 rounded-lg border border-transparent px-2.5 text-xs transition ${
                        isSearching && isSearchBarCompact ? "h-8" : "h-9"
                      } ${
                        scopeMenuOpen
                          ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                          : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
                      }`}
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
                            <span className="text-[11px] tracking-[0.08em] text-[var(--text-secondary)]">
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
                    ref={searchInputRef}
                    type="text"
                    autoFocus
                    disabled={modelSetupNotConfigured}
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={onKeyDown}
                    onFocus={() => {
                      setIsSearchInputFocused(true);
                      if (isSearchBarCompact) {
                        setIsSearchBarHovering(true);
                      }
                      if (fileMatchesCloseTimerRef.current != null) {
                        window.clearTimeout(fileMatchesCloseTimerRef.current);
                        fileMatchesCloseTimerRef.current = null;
                      }
                    }}
                    onBlur={() => {
                      setIsSearchInputFocused(false);
                      if (isSearchBarCompact && !scopeMenuOpen) {
                        setIsSearchBarHovering(false);
                      }
                      fileMatchesCloseTimerRef.current = window.setTimeout(() => {
                        setFileMatchesOpen(false);
                      }, 120);
                    }}
                    placeholder={searchPlaceholder}
                    className={`w-full flex-1 border-none bg-transparent pr-10 text-xl text-[var(--text-primary)] focus:outline-none focus:ring-0 disabled:cursor-not-allowed disabled:opacity-100 ${
                      modelSetupNotConfigured
                        ? "placeholder:text-red-400"
                        : "placeholder:text-[var(--text-muted)]"
                    }`}
                  />
                </div>
                  <AnimatePresence>
                    {fileMatchesOpen && fileMatches.length > 0 && (
                      <motion.div
                        initial={{ opacity: 0, y: -6 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -6 }}
                        transition={{ duration: 0.18, ease: "easeOut" }}
                        onMouseDown={(event) => event.preventDefault()}
                      className="absolute left-0 right-0 top-full z-50 mt-2 overflow-hidden rounded-xl bg-[var(--bg-surface-1)] ring-1 ring-[var(--line-soft)] shadow-[var(--float-shadow)]"
                      >
                        <div className="px-3 py-2 text-[11px] text-[var(--text-muted)]">
                        {uiLang === "zh-CN" ? "相关文件" : "Relevant files"}
                        </div>
                      <div className="settings-scrollbar max-h-72 overflow-y-auto pr-1">
                        {fileMatches.slice(0, 20).map((item) => {
                          const isSelected = selectedScopeSet.has(item.file_path);
                          const parent = item.parent_dir || "";
                          const relative =
                            watchRoot && parent.toLowerCase().startsWith(watchRoot.toLowerCase())
                              ? parent.slice(watchRoot.length).replace(/^[/\\\\]/, "")
                              : parent;
                          return (
                            <button
                              key={item.file_path}
                              type="button"
                              aria-pressed={isSelected}
                              onClick={() => {
                                setSelectedScopePaths((prev) => {
                                  if (prev.includes(item.file_path)) {
                                    return prev.filter((p) => p !== item.file_path);
                                  }
                                  return [...prev, item.file_path];
                                });
                                // Selecting a file here should only set scope, not trigger a query.
                                requestAnimationFrame(() => searchInputRef.current?.focus());
                              }}
                              className={`group flex w-full items-center gap-3 px-3 py-2 text-left transition-[background-color,transform] duration-180 ease-out active:scale-[0.995] ${
                                isSelected
                                  ? "bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]"
                                  : "hover:bg-[color-mix(in_srgb,var(--accent)_6%,transparent)]"
                              }`}
                            >
                              <FileText
                                className={`h-4 w-4 shrink-0 ${
                                  isSelected ? "text-[var(--accent)]" : "text-[var(--text-secondary)]"
                                }`}
                              />
                              <span className="min-w-0 flex-1">
                                <span className="block truncate text-sm text-[var(--text-primary)]">
                                  {item.file_name || item.file_path}
                                </span>
                                <span className="block truncate text-[11px] text-[var(--text-muted)]">
                                  {relative || item.parent_dir}
                                </span>
                              </span>
                              <span
                                className={`ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded-full transition-colors duration-180 ease-out ${
                                  isSelected
                                    ? "bg-[color-mix(in_srgb,var(--accent)_18%,transparent)] text-[var(--accent)]"
                                    : "bg-[color-mix(in_srgb,var(--text-muted)_6%,transparent)] text-[color-mix(in_srgb,var(--text-muted)_20%,transparent)] group-hover:bg-[color-mix(in_srgb,var(--accent)_8%,transparent)]"
                                }`}
                              >
                                <Check
                                  className={`h-3.5 w-3.5 transition-[opacity,transform] duration-180 ease-out ${
                                    isSelected ? "opacity-100 scale-100" : "opacity-0 scale-75"
                                  }`}
                                />
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
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
                  </>
                )}
              </motion.div>
            </motion.div>

            <AnimatePresence>
              {isSearching && (
                <motion.section
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 12 }}
                  transition={{ duration: 0.24, ease: "easeOut" }}
                  style={{
                    paddingTop: isSearchBarCollapsed ? 74 : isSearchBarCompact ? 102 : 146,
                    transition: "padding-top 0.24s ease-out"
                  }}
                  className="no-scrollbar mx-auto h-full w-full max-w-4xl overflow-y-auto"
                  onScroll={onResultScroll}
                  onWheel={onResultWheel}
                >
                  {loading && (
                    <div className="flex items-center justify-between px-1 py-3 text-sm text-[var(--text-secondary)]">
                      <div className="flex items-center gap-3">
                        <LoaderCircle className="h-4 w-4 animate-spin text-[var(--accent)]" />
                        {t("loading")}
                      </div>
                      <span className="text-xs text-[var(--text-muted)]">
                        {formatElapsed(searchElapsedMs)}
                      </span>
                    </div>
                  )}

                  {!loading && error && (
                    <div className="rounded-xl border border-red-500/40 bg-red-500/10 px-5 py-4 text-sm text-red-300">
                      {error}
                    </div>
                  )}

                  {!loading && !error && answerResponse && (
                    <article className="pb-8">
                      <div className={`mb-5 rounded-xl border px-4 py-3 text-sm ${statusToneClasses(answerResponse.status)}`}>
                        <div className="flex items-center justify-between gap-3">
                          <div className="flex items-center gap-2">
                            <Sparkles className="h-4 w-4" />
                            <span className="font-semibold">
                              {answerResponse.status === "answered"
                                ? t("answerStatusAnswered")
                                : answerResponse.status === "model_failed_with_evidence"
                                  ? t("answerStatusModelFailed")
                                  : t("answerStatusInsufficient")}
                            </span>
                          </div>
                          {lastSearchDurationMs !== null ? (
                            <span className="text-[11px] text-[var(--text-secondary)]">
                              {t("elapsedTime", { time: formatElapsed(lastSearchDurationMs) })}
                            </span>
                          ) : null}
                        </div>
                        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-[var(--text-secondary)]">
                          <span>{t("docCandidateCount", { count: answerResponse.metrics.doc_candidate_count })}</span>
                          <span>{t("chunkCandidateCount", { count: answerResponse.metrics.chunk_candidate_count })}</span>
                          <span>{t("finalEvidenceCount", { count: answerResponse.metrics.final_evidence_count })}</span>
                        </div>
                      </div>

                      {answerResponse.answer.trim().length > 0 && (
                        <div className="mb-6 border-l-2 border-[var(--accent)] pl-4">
                          <div className="mb-3 flex items-center gap-2">
                            <Atom className="h-4 w-4 text-[var(--accent)]" />
                            <span className="text-xs font-bold tracking-widest text-[var(--accent)]">
                              {t("synthesis")}
                            </span>
                          </div>
                          <div className="md-preview mt-1 break-words font-sans text-lg leading-relaxed text-[var(--text-primary)]">
                            <ReactMarkdown
                              remarkPlugins={MARKDOWN_REMARK_PLUGINS}
                              rehypePlugins={MARKDOWN_REHYPE_PLUGINS}
                            >
                              {answerResponse.answer}
                            </ReactMarkdown>
                          </div>
                        </div>
                      )}

                      {visibleCitations.length > 0 && (
                        <section className="mt-6">
                          <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
                            {t("citationsTitle")}
                          </div>
                          <div className="space-y-3">
                            {visibleCitations.map((citation) => (
                              (() => {
                                const expanded = expandedCitationKeys.has(citation.citation_key);
                                const citationContent = expanded
                                  ? citation.excerpt
                                  : buildCollapsedMarkdownPreview(citation.excerpt, 5);

                                return (
                                  <div
                                    key={citation.citation_key}
                                    className="relative overflow-hidden rounded-xl border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-4 py-3"
                                  >
                                    <div
                                      aria-hidden="true"
                                      className="pointer-events-none absolute -left-1 -top-3 z-0 select-none italic text-[88px] font-semibold leading-none text-[color-mix(in_srgb,var(--accent)_16%,transparent)]"
                                    >
                                      {citation.index}
                                    </div>
                                    <div className="relative z-10 mb-2 flex items-start justify-between gap-3">
                                      <div className="min-w-0">
                                        <div className="text-xs font-semibold text-[var(--accent)]">
                                          {citation.relative_path || citation.file_path}
                                        </div>
                                        {citation.heading_path.length > 0 ? (
                                          <div className="mt-1 text-[11px] text-[var(--text-secondary)]">
                                            {citation.heading_path.join(" > ")}
                                          </div>
                                        ) : null}
                                        {citation.duplicate_count > 1 ? (
                                          <div className="mt-2 inline-flex rounded-full border border-[var(--border-strong)] px-2 py-0.5 text-[10px] tracking-[0.08em] text-[var(--text-secondary)] uppercase">
                                            {t("citationDuplicates", { count: citation.duplicate_count })}
                                          </div>
                                        ) : null}
                                      </div>
                                      <div className="flex items-center gap-2">
                                        <button
                                          type="button"
                                          onClick={() => void onOpenSourceLocation(citation.file_path)}
                                          className="p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                          aria-label={t("openSourceLocation")}
                                          title={t("openSourceLocation")}
                                        >
                                          <FolderOpen className="h-4 w-4" />
                                        </button>
                                        <button
                                          type="button"
                                          onClick={() => toggleCitationExpanded(citation.citation_key)}
                                          className="p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                          aria-label={expanded ? t("collapseCitation") : t("expandCitation")}
                                          title={expanded ? t("collapseCitation") : t("expandCitation")}
                                        >
                                          <motion.span
                                            animate={{ rotate: expanded ? 180 : 0 }}
                                            transition={{ duration: 0.2, ease: "easeOut" }}
                                            className="inline-flex"
                                          >
                                            <ChevronDown className="h-4 w-4" />
                                          </motion.span>
                                        </button>
                                      </div>
                                    </div>
                                    <div
                                      className={`relative z-10 md-preview md-preview-source text-sm leading-6 text-[var(--text-secondary)] ${
                                        !expanded
                                          ? "source-preview-scrollbar max-h-28 overflow-y-auto pr-2"
                                          : ""
                                      }`}
                                    >
                                      <ReactMarkdown
                                        remarkPlugins={MARKDOWN_REMARK_PLUGINS}
                                        rehypePlugins={MARKDOWN_REHYPE_PLUGINS}
                                      >
                                        {expanded ? citation.excerpt : citationContent}
                                      </ReactMarkdown>
                                    </div>
                                  </div>
                                );
                              })()
                            ))}
                          </div>
                        </section>
                      )}

                      {visibleEvidenceGroups.length > 0 && (
                        <section className="mt-8">
                          <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
                            {t("evidenceTitle")}
                          </div>

                          <div className="grid grid-cols-1 items-stretch gap-3 md:grid-cols-2">
                            {visibleEvidenceGroups.map((source) => {
                              const sourceKey = source.evidence_key;
                              const expanded = expandedSourceKeys.has(sourceKey);
                              const markdownPreview = isMarkdownFile(source.file_path);
                              const markdownContent = expanded
                                ? source.content
                                : buildCollapsedMarkdownPreview(source.content, 7);

                              return (
                                <div
                                  key={sourceKey}
                                  className={`relative flex h-full flex-col rounded-xl border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-4 py-3 ${
                                    expanded ? "md:col-span-2" : ""
                                  }`}
                                >
                                  <div className="min-w-0 flex-1 pr-12">
                                    <div className="flex flex-wrap items-center gap-2 text-[11px]">
                                      {source.document_reasons.map((reason) => (
                                        <span
                                          key={`doc-${sourceKey}-${reason}`}
                                          className="rounded-full bg-[var(--bg-surface-2)] px-2 py-0.5 text-[var(--text-secondary)]"
                                        >
                                          {formatDocumentReason(reason, t)}
                                        </span>
                                      ))}
                                      {source.reasons.map((reason) => (
                                        <span
                                          key={`reason-${sourceKey}-${reason}`}
                                          className="rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent)]"
                                        >
                                          {formatEvidenceReason(reason, t)}
                                        </span>
                                      ))}
                                      <span className="text-[var(--text-secondary)]">
                                        {t("documentRankLabel", { count: source.document_rank })}
                                      </span>
                                      <span className="text-[var(--text-secondary)]">
                                        {t("chunkRankLabel", { count: source.top_chunk_rank })}
                                      </span>
                                      <span className="text-[var(--text-muted)]">
                                        {t("evidenceFragments", { count: source.fragment_count })}
                                      </span>
                                    </div>
                                    <div className="mt-2 truncate font-mono text-xs text-[var(--text-secondary)]" title={source.file_path}>
                                      {source.relative_path || source.file_path}
                                    </div>
                                    <div className="mt-1 text-[11px] text-[var(--text-muted)]">
                                      {source.block_kinds.join(" / ")}
                                      {source.heading_paths.length > 0 ? ` · ${source.heading_paths.join(" · ")}` : ""}
                                    </div>
                                    {markdownPreview ? (
                                      <div
                                        className={`md-preview md-preview-source mt-2 text-sm leading-6 text-[var(--text-secondary)] ${
                                          !expanded
                                            ? "source-preview-scrollbar max-h-24 overflow-y-auto pr-2"
                                            : ""
                                        }`}
                                      >
                                        <ReactMarkdown
                                          remarkPlugins={MARKDOWN_REMARK_PLUGINS}
                                          rehypePlugins={MARKDOWN_REHYPE_PLUGINS}
                                        >
                                          {expanded ? source.content : markdownContent}
                                        </ReactMarkdown>
                                      </div>
                                    ) : (
                                      <div
                                        className={`mt-2 whitespace-pre-wrap break-words font-mono text-[13px] leading-6 text-[var(--text-muted)] ${
                                          !expanded
                                            ? "source-preview-scrollbar max-h-24 overflow-y-auto pr-2"
                                            : ""
                                        }`}
                                      >
                                        {expanded ? source.content : markdownContent}
                                      </div>
                                    )}
                                  </div>

                                  <button
                                    type="button"
                                    onClick={() => void onOpenSourceLocation(source.file_path)}
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

                      <section className="mt-8">
                        <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
                          {t("retrievalMetricsTitle")}
                        </div>
                        {answerResponse.metrics.query_flags.length > 0 ? (
                          <div className="mb-3 flex flex-wrap gap-2 text-[11px]">
                            {answerResponse.metrics.query_flags.map((flag) => (
                              <span
                                key={flag}
                                className="rounded-full border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-0.5 text-[var(--text-secondary)]"
                              >
                                {formatQueryFlag(flag, t)}
                              </span>
                            ))}
                          </div>
                        ) : null}
                        <div className="mb-4 flex flex-wrap items-center gap-x-5 gap-y-1 text-xs text-[var(--text-secondary)]">
                          <span>
                            {t("metricTotal")}{" "}
                            <span className="font-mono text-[var(--text-primary)]">
                              {lastSearchDurationMs !== null ? formatElapsed(lastSearchDurationMs) : "-"}
                            </span>
                          </span>
                          <span>
                            {t("metricMeasured")}{" "}
                            <span className="font-mono text-[var(--text-primary)]">
                              {formatMetricDuration(measuredMetricsTotalMs)}
                            </span>
                          </span>
                          {lastSearchDurationMs !== null && lastSearchDurationMs > measuredMetricsTotalMs ? (
                            <span>
                              {t("metricUntracked")}{" "}
                              <span className="font-mono text-[var(--text-primary)]">
                                {formatMetricDuration(lastSearchDurationMs - measuredMetricsTotalMs)}
                              </span>
                            </span>
                          ) : null}
                        </div>
                        <div className="space-y-3">
                          {metricRows.map((metric, index) => {
                            const maxMetricValue = metricRows[0]?.value ?? 1;
                            const widthPercent = Math.max(6, (metric.value / maxMetricValue) * 100);
                            return (
                              <div
                                key={metric.key}
                                className="flex items-center gap-3 text-xs text-[var(--text-secondary)]"
                              >
                                <span className="w-6 shrink-0 text-right font-mono text-[10px] text-[var(--text-muted)]">
                                  {index + 1}
                                </span>
                                <span className="w-28 shrink-0 truncate text-[var(--text-secondary)] md:w-36">
                                  {metric.label}
                                </span>
                                <div className="min-w-0 flex-1">
                                  <div className="h-1.5 w-full overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--accent)_10%,var(--bg-canvas)_90%)]">
                                    <div
                                      className="h-full rounded-full bg-[var(--accent)]"
                                      style={{ width: `${widthPercent}%` }}
                                    />
                                  </div>
                                </div>
                                <span className="w-14 shrink-0 text-right font-mono text-[var(--text-primary)] md:w-16">
                                  {formatMetricDuration(metric.value)}
                                </span>
                              </div>
                            );
                          })}
                        </div>
                        <div className="mt-3 text-[11px] text-[var(--text-muted)]">
                          {t("metricsNote")}
                        </div>
                      </section>
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
            <span className="inline-flex items-center gap-2 text-[var(--accent)]">
              <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent)]" />
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
