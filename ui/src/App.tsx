import {
  KeyboardEvent as ReactKeyboardEvent,
  UIEvent as ReactUIEvent,
  WheelEvent as ReactWheelEvent,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ChevronLeft, ChevronRight } from "lucide-react";
import rehypeHighlight from "rehype-highlight";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { SettingsModal } from "./components/SettingsModal";
import type {
  EnterprisePolicyDto,
  FontPreset,
  FontScale,
  IndexFilterConfigDto,
  IndexingMode,
  IndexingStatusDto,
  McpSettingsDto,
  McpStatusDto,
  MemorySettingsDto,
  ModelAvailabilityDto,
  ModelSettingsDto,
  LocalModelRuntimeStatusesDto,
  ProviderModelsDto,
  ResourceBudget,
  ThemeMode
} from "./components/settings/types";
import { useI18n } from "./i18n";
import type { Language } from "./i18n";
import { buildVisibleCitations, buildVisibleEvidenceGroups } from "./app/evidence";
import {
  buildCollapsedMarkdownPreview,
  formatElapsed,
  formatMetricDuration,
  statusToneClasses
} from "./app/formatters";
import { useQueryFlow } from "./app/hooks/useQueryFlow";
import { useAppearanceSync } from "./app/hooks/useAppearanceSync";
import { useScopeManager } from "./app/hooks/useScopeManager";
import { useWindowControls } from "./app/hooks/useWindowControls";
import { FilePreview } from "./app/layout/FilePreview";
import { GraphView } from "./app/layout/GraphView";
import { OnboardingOverlay } from "./app/layout/OnboardingOverlay";
import { ResultStage } from "./app/layout/ResultStage";
import { SearchStage } from "./app/layout/SearchStage";
import { Sidebar } from "./app/layout/Sidebar";
import { StatusFooter } from "./app/layout/StatusFooter";
import { TitleBar } from "./app/layout/TitleBar";
import type {
  AppSettingsDto,
  AskResponseStructured,
  FileMatch,
  MetricRow,
  VaultStats,
  VaultStatsRaw,
  VisibleCitation,
  VisibleEvidenceGroup
} from "./app/types";
import {
  AI_LANG_STORAGE_KEY,
  DEFAULT_ENTERPRISE_POLICY,
  DEFAULT_FILTER_CONFIG,
  DEFAULT_FONT_SCALE,
  DEFAULT_MEMORY_SETTINGS,
  DEFAULT_MCP_SETTINGS,
  DEFAULT_MODEL_SETTINGS,
  DEFAULT_SIDEBAR_WIDTH,
  FONT_PRESET_STORAGE_KEY,
  FONT_SCALE_STORAGE_KEY,
  INDEXING_ACTION_TIMEOUT_MS,
  LEGACY_THEME_MODE_STORAGE_KEY,
  LOCAL_MODEL_ACTION_TIMEOUT_MS,
  MARKDOWN_REHYPE_PLUGINS,
  MARKDOWN_REMARK_PLUGINS,
  MAX_SIDEBAR_WIDTH,
  MIN_SIDEBAR_WIDTH,
  MODEL_ACTION_TIMEOUT_MS,
  MODEL_NOT_CONFIGURED_CODE,
  RETRIEVE_TOP_K_STORAGE_KEY,
  SIDEBAR_WIDTH_STORAGE_KEY,
  THEME_STORAGE_KEY,
  TAURI_HOST_MISSING_MESSAGE,
  isTauriHostAvailable,
  normalizeIndexingMode,
  normalizeResourceBudget,
  normalizeStats,
  resolveInitialAiLanguage,
  resolveInitialFontPreset,
  resolveInitialFontScale,
  resolveInitialRetrieveTopK,
  resolveInitialSidebarWidth,
  resolveInitialThemeMode,
  settingsToMemorySettings,
  toUiErrorMessage,
  withTimeout
} from "./app/app-helpers";
import { useAppInit } from "./app/useAppInit";
import { useAppEffects } from "./app/useAppEffects";
import { useAppIndex } from "./app/useAppIndex";
import { useAppModel } from "./app/useAppModel";
import { useAppSettings } from "./app/useAppSettings";
import { useAppUI } from "./app/useAppUI";

export default function App() {
  const { lang: uiLang, setLang: setUiLang, t } = useI18n();
  const [isSearchBarCompact, setIsSearchBarCompact] = useState(false);
  const [isSearchBarHovering, setIsSearchBarHovering] = useState(false);
  const [isSearchInputFocused, setIsSearchInputFocused] = useState(false);
  const [allowCompactHoverExpand, setAllowCompactHoverExpand] = useState(true);
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
  const [fileMatches, setFileMatches] = useState<FileMatch[]>([]);
  const [fileMatchesOpen, setFileMatchesOpen] = useState(false);
  const [expandedSourceKeys, setExpandedSourceKeys] = useState<Set<string>>(() => new Set());
  const [expandedCitationKeys, setExpandedCitationKeys] = useState<Set<string>>(() => new Set());
  const [modelSettings, setModelSettings] = useState<ModelSettingsDto>(DEFAULT_MODEL_SETTINGS);
  const [enterprisePolicy, setEnterprisePolicy] =
    useState<EnterprisePolicyDto>(DEFAULT_ENTERPRISE_POLICY);
  const [mcpSettings, setMcpSettings] = useState<McpSettingsDto>(DEFAULT_MCP_SETTINGS);
  const [mcpStatus, setMcpStatus] = useState<McpStatusDto | null>(null);
  const [mcpBusy, setMcpBusy] = useState(false);
  const [mcpMessage, setMcpMessage] = useState<string | null>(null);
  const [memorySettings, setMemorySettings] = useState<MemorySettingsDto>(DEFAULT_MEMORY_SETTINGS);
  const [memoryBusy, setMemoryBusy] = useState(false);
  const [memoryMessage, setMemoryMessage] = useState<string | null>(null);
  const [filterConfig, setFilterConfig] = useState<IndexFilterConfigDto>({
    enabled: false,
    include_extensions: [],
    exclude_extensions: [],
    exclude_paths: [],
    include_paths: [],
    min_mtime: null,
    max_mtime: null,
    min_size: null,
    max_size: null,
  });
  const [filterBusy, setFilterBusy] = useState(false);
  const [filterMessage, setFilterMessage] = useState<string | null>(null);
  const [modelAvailability, setModelAvailability] = useState<ModelAvailabilityDto | null>(null);
  const [localModelRuntimeStatuses, setLocalModelRuntimeStatuses] =
    useState<LocalModelRuntimeStatusesDto | null>(null);
  const [localModelRuntimeBusyRole, setLocalModelRuntimeBusyRole] = useState<string | null>(null);
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
  const [previewFilePath, setPreviewFilePath] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string | null>(null);
  const [previewFormat, setPreviewFormat] = useState<string>("text");
  const [sidebarWidth, setSidebarWidth] = useState<number>(() => resolveInitialSidebarWidth());
  const sidebarWidthRef = useRef<number>(sidebarWidth);
  useEffect(() => { sidebarWidthRef.current = sidebarWidth; }, [sidebarWidth]);
  const [graphViewOpen, setGraphViewOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const compactHoverUnlockTimerRef = useRef<number | null>(null);
  const fileMatchesCloseTimerRef = useRef<number | null>(null);
  const reachedTopWhileCompactRef = useRef(false);

  const {
    scopeMenuOpen,
    setScopeMenuOpen,
    scopeMenuRef,
    scopeItems,
    scopeLoading,
    selectedScopePaths,
    setSelectedScopePaths,
    selectedScopeSet,
    expandedScopeDirs,
    setExpandedScopeDirs,
    scopeChildrenCountByParentKey,
    visibleScopeItems,
    onToggleScopePath,
    onClearScopeSelection,
    onToggleScopeDirExpanded
  } = useScopeManager({
    watchRoot,
    toUiErrorMessage,
    onError: setError
  });

  const { isMaximized, onMinimize, onToggleMaximize, onClose } = useWindowControls({
    toUiErrorMessage,
    onError: setError
  });

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
      modelSettings.active_provider === "llama_cpp_local"
        ? modelSettings.local_profile
        : modelSettings.remote_profile,
    [modelSettings]
  );

  useAppearanceSync({
    aiLang,
    themeMode,
    fontPreset,
    fontScale,
    retrieveTopK,
    aiLangStorageKey: AI_LANG_STORAGE_KEY,
    themeStorageKey: THEME_STORAGE_KEY,
    legacyThemeModeStorageKey: LEGACY_THEME_MODE_STORAGE_KEY,
    fontPresetStorageKey: FONT_PRESET_STORAGE_KEY,
    fontScaleStorageKey: FONT_SCALE_STORAGE_KEY,
    retrieveTopKStorageKey: RETRIEVE_TOP_K_STORAGE_KEY
  });

  useAppInit({
    setStats,
    setWatchRoot,
    setIndexingMode,
    setResourceBudget,
    setScheduleStart,
    setScheduleEnd,
    setMemorySettings,
    setFilterConfig,
    setAiLang,
    setEnterprisePolicy,
    setMcpSettings,
    setMcpStatus,
    setModelSettings,
    setProviderModels,
    setLocalModelRuntimeStatuses,
    setModelAvailability,
    setIndexingStatus,
    setError
  });

  useAppEffects({
    isOnboardingOpen,
    isSettingsOpen,
    setIsSettingsOpen,
    setIsOnboardingOpen,
    modelSettings,
    setProviderModels,
    setFileMatchesOpen,
    setFileMatches,
    isSearching,
    isSearchBarCompact,
    setIsSearchBarCompact,
    setIsSearchBarHovering,
    setIsSearchInputFocused,
    setAllowCompactHoverExpand,
    isSearchInputFocused,
    scopeMenuOpen,
    query,
    isSearchBarCollapsed,
    setError,
    compactHoverUnlockTimerRef
  });

  const {
    refreshIndexingStatus,
    onSaveIndexingConfig,
    onTriggerReindex,
    onPauseIndexing,
    onResumeIndexing
  } = useAppIndex({
    indexingMode,
    resourceBudget,
    scheduleStart,
    scheduleEnd,
    setIndexingMode,
    setResourceBudget,
    setScheduleStart,
    setScheduleEnd,
    setIndexingStatus,
    setIndexingBusy,
    setError
  });

  const {
    onProbeModelProvider,
    onRefreshProviderModels,
    onSaveModelSettings,
    onRefreshLocalModelRuntimeStatus,
    onStartLocalModel,
    onStopLocalModel,
    onRestartLocalModel
  } = useAppModel({
    modelSettings,
    activeModelProfile,
    uiLang,
    setModelBusy,
    setError,
    setModelAvailability,
    setProviderModels,
    setModelSettings,
    setLocalModelRuntimeStatuses,
    setLocalModelRuntimeBusyRole,
    setIsOnboardingOpen
  });

  const {
    onSaveEnterprisePolicy,
    onSelectProvider,
    onPickLocalModelsRoot,
    onClearLocalModelsRoot,
    onSaveMcpSettings,
    onCopyMcpClientConfig,
    onSaveMemorySettings,
    onSaveFilterConfig
  } = useAppSettings({
    enterprisePolicy,
    modelSettings,
    mcpSettings,
    memorySettings,
    filterConfig,
    uiLang,
    setEnterpriseBusy,
    setEnterprisePolicy,
    setModelAvailability,
    setError,
    setModelSettings,
    setProviderModels,
    setMcpBusy,
    setMcpMessage,
    setMcpSettings,
    setMcpStatus,
    setMemoryBusy,
    setMemoryMessage,
    setMemorySettings,
    setFilterBusy,
    setFilterMessage,
    refreshIndexingStatus
  });

  const {
    onToggleThemeMode,
    onKeyDown,
    updateActiveOnboardingProfile,
    toggleSourceExpanded,
    toggleCitationExpanded,
    onOpenSourceLocation,
    onPreviewFile,
    onCloseFilePreview,
    onPickWatchRoot,
    onResultScroll,
    onResultWheel,
    onSidebarResizeStart
  } = useAppUI({
    setThemeMode,
    runSearch,
    setModelSettings,
    setExpandedSourceKeys,
    setExpandedCitationKeys,
    setError,
    setPreviewFilePath,
    setPreviewContent,
    setPreviewFormat,
    setIsPickingWatchRoot,
    watchRoot,
    setWatchRoot,
    setSelectedScopePaths,
    setExpandedScopeDirs,
    setStats,
    isSearchBarCompact,
    allowCompactHoverExpand,
    scopeMenuOpen,
    isSearchBarHovering,
    isSearchInputFocused,
    setIsSearchBarCompact,
    setAllowCompactHoverExpand,
    setIsSearchBarHovering,
    setIsSearchInputFocused,
    setScopeMenuOpen,
    searchInputRef,
    compactHoverUnlockTimerRef,
    reachedTopWhileCompactRef,
    sidebarWidthRef,
    setSidebarWidth
  });

  return (
    <div className="h-screen w-screen bg-[var(--bg-canvas)] text-[var(--text-primary)]">
      <div className="relative flex h-full w-full flex-col overflow-hidden bg-[var(--bg-canvas)]">
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

        <div className="relative z-10 flex flex-1 overflow-hidden">
          <div style={{ width: sidebarWidth }} className="shrink-0 h-full overflow-hidden">
            <Sidebar
              t={t}
              watchRoot={watchRoot}
              scopeItems={scopeItems}
              scopeLoading={scopeLoading}
              expandedScopeDirs={expandedScopeDirs}
              stats={stats}
              onToggleScopeDirExpanded={onToggleScopeDirExpanded}
              onPreviewFile={onPreviewFile}
              onToggleSettings={() => setIsSettingsOpen((prev) => !prev)}
            />
          </div>

          {/* Sidebar resizer */}
          <div
            className="relative z-20 w-[5px] shrink-0 cursor-col-resize hover:bg-[var(--accent-soft)] active:bg-[var(--accent)] transition-colors"
            onMouseDown={onSidebarResizeStart}
          />

          <div className="relative flex flex-1 overflow-hidden">
            {/* Search panel */}
            <motion.div
              className="absolute inset-0 z-10 flex flex-col overflow-hidden"
              animate={{ x: graphViewOpen ? "-100%" : 0 }}
              transition={{ type: "spring", stiffness: 300, damping: 30 }}
            >
              <div
                className={`pointer-events-none absolute inset-0 z-0 ${
                  themeMode === "dark"
                    ? "bg-[radial-gradient(104%_74%_at_50%_-10%,rgba(181,210,238,0.17),transparent_64%),radial-gradient(70%_52%_at_18%_34%,rgba(111,154,200,0.1),transparent_76%),radial-gradient(66%_50%_at_84%_36%,rgba(104,145,189,0.09),transparent_78%),linear-gradient(180deg,#121b28_0%,#111926_46%,#101722_100%)]"
                    : "bg-[radial-gradient(120%_80%_at_50%_-10%,rgba(255,255,255,0.82),transparent_62%),radial-gradient(72%_54%_at_18%_30%,rgba(196,220,244,0.42),transparent_78%),radial-gradient(68%_52%_at_84%_34%,rgba(205,225,244,0.36),transparent_80%),linear-gradient(180deg,#f8fbff_0%,#f4f8fd_56%,#f2f6fb_100%)]"
                }`}
              />

              <main className="relative z-10 flex flex-1 flex-col overflow-hidden">
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

                <div className="flex-1 overflow-y-auto">
                  {previewFilePath && previewContent !== null ? (
                    <FilePreview
                      t={t}
                      filePath={previewFilePath}
                      content={previewContent}
                      format={previewFormat}
                      onClose={onCloseFilePreview}
                    />
                  ) : (
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
                  )}
                </div>
              </main>

              <StatusFooter t={t} stats={stats} />
            </motion.div>

            {/* Edge toggle button */}
            <button
              type="button"
              onClick={() => setGraphViewOpen((prev) => !prev)}
              className="absolute right-0 top-1/2 z-30 flex -translate-y-1/2 flex-col items-center gap-1 rounded-l-lg border border-r-0 border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-1.5 py-3 text-xs text-[var(--text-secondary)] shadow-sm transition hover:bg-[var(--bg-surface-2)] hover:text-[var(--accent)]"
            >
              {graphViewOpen ? (
                <>
                  <ChevronRight className="h-3.5 w-3.5" />
                  <span className="[writing-mode:vertical-rl]">搜索</span>
                </>
              ) : (
                <>
                  <ChevronLeft className="h-3.5 w-3.5" />
                  <span className="[writing-mode:vertical-rl]">图谱</span>
                </>
              )}
            </button>

            {/* Graph panel */}
            <motion.div
              className="absolute inset-0 z-20 flex flex-col overflow-hidden bg-[var(--bg-canvas)]"
              initial={{ x: "100%" }}
              animate={{ x: graphViewOpen ? 0 : "100%" }}
              transition={{ type: "spring", stiffness: 300, damping: 30 }}
            >
              <GraphView />
            </motion.div>
          </div>
        </div>

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
                localModelRuntimeStatuses={localModelRuntimeStatuses}
                localModelRuntimeBusyRole={localModelRuntimeBusyRole}
                onRefreshLocalModelRuntimeStatus={onRefreshLocalModelRuntimeStatus}
                onStartLocalModel={onStartLocalModel}
                onStopLocalModel={onStopLocalModel}
                onRestartLocalModel={onRestartLocalModel}
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
                mcpSettings={mcpSettings}
                mcpStatus={mcpStatus}
                mcpBusy={mcpBusy}
                mcpMessage={mcpMessage}
                onMcpSettingsChange={setMcpSettings}
                onSaveMcpSettings={onSaveMcpSettings}
                onCopyMcpClientConfig={onCopyMcpClientConfig}
                memorySettings={memorySettings}
                memoryBusy={memoryBusy}
                memoryMessage={memoryMessage}
                onMemorySettingsChange={setMemorySettings}
                onSaveMemorySettings={onSaveMemorySettings}
                filterConfig={filterConfig}
                filterBusy={filterBusy}
                filterMessage={filterMessage}
                onFilterConfigChange={setFilterConfig}
                onSaveFilterConfig={onSaveFilterConfig}
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
        />
      </div>
    </div>
  );
}
