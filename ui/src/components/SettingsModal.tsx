import { AnimatePresence, motion } from "framer-motion";
import { ArrowRight, Brain, Cpu, Database, LoaderCircle, Network, Palette, ScrollText, Search, Settings } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Language } from "../i18n";
import { useI18n } from "../i18n";
import {
  AnimatedPanel,
  AnimatedPressButton,
  fadeSlideUpVariants,
  staggerContainerVariants
} from "./MotionKit";
import { rankSettingsQuery } from "../app/api/desktop";
import { AdvancedTab, BasicTab, LogsTab, McpTab, MemoryTab, ModelsTab, PersonalizationTab } from "./settings/tabs";
import type {
  LocalModelProfileDto,
  RemoteModelProfileDto,
  FontPreset,
  FontScale,
  IndexingMode,
  ModelAvailabilityDto,
  ModelProvider,
  ModelRole,
  ResourceBudget,
  SettingsModalProps
} from "./settings/types";
import type { IndexingActionKey } from "./settings/tabs/AdvancedTab";
import type { ModelActionKey } from "./settings/tabs/ModelsTab";

type TabKey = "basic" | "models" | "memory" | "mcp" | "advanced" | "personalization" | "logs";

export function SettingsModal({
  open,
  onBack,
  uiLang,
  aiLang,
  onUiLangChange,
  onAiLangChange,
  watchRoot,
  isPickingWatchRoot,
  onPickWatchRoot,
  retrieveTopK,
  onRetrieveTopKChange,
  fontPreset,
  onFontPresetChange,
  fontScale,
  onFontScaleChange,
  themeMode,
  onThemeModeChange,
  modelSettings,
  enterprisePolicy,
  modelAvailability,
  providerModels,
  modelBusy,
  enterpriseBusy,
  onModelSettingsChange,
  onEnterprisePolicyChange,
  onSaveModelSettings,
  onSaveEnterprisePolicy,
  onProbeModelProvider,
  onRefreshProviderModels,
  onPullModel,
  onPickLocalModelsRoot,
  onClearLocalModelsRoot,
  indexingMode,
  resourceBudget,
  scheduleStart,
  scheduleEnd,
  indexingStatus,
  indexingBusy,
  onIndexingModeChange,
  onResourceBudgetChange,
  onScheduleStartChange,
  onScheduleEndChange,
  onSaveIndexingConfig,
  onTriggerReindex,
  onPauseIndexing,
  onResumeIndexing,
  mcpSettings,
  mcpStatus,
  mcpBusy,
  mcpMessage,
  onMcpSettingsChange,
  onSaveMcpSettings,
  onCopyMcpClientConfig,
  memorySettings,
  memoryBusy,
  memoryMessage,
  onMemorySettingsChange,
  onSaveMemorySettings
}: SettingsModalProps) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const [search, setSearch] = useState("");
  const [aiMatchedKeys, setAiMatchedKeys] = useState<TabKey[] | null>(null);
  const [autoSyncDaemon, setAutoSyncDaemon] = useState(true);
  const [graphRagInfer, setGraphRagInfer] = useState(true);
  type ActionPhase = "idle" | "running" | "success" | "error";
  const [actionState, setActionState] = useState<Record<ModelActionKey, { phase: ActionPhase; tick: number }>>({
    probe: { phase: "idle", tick: 0 },
    refresh: { phase: "idle", tick: 0 },
    save: { phase: "idle", tick: 0 },
    pull: { phase: "idle", tick: 0 }
  });
  const resetTimersRef = useRef<Record<ModelActionKey, number | null>>({
    probe: null,
    refresh: null,
    save: null,
    pull: null
  });
  const [customMode, setCustomMode] = useState<Record<ModelRole, boolean>>({
    chat_model: false,
    graph_model: false,
    embed_model: false
  });
  const [indexingAction, setIndexingAction] = useState<{
    key: IndexingActionKey | null;
    phase: ActionPhase;
    tick: number;
  }>({
    key: null,
    phase: "idle",
    tick: 0
  });

  const activeProvider = modelSettings.active_provider;
  const activeProfile =
    activeProvider === "ollama_local" ? modelSettings.local_profile : modelSettings.remote_profile;
  const normalizePolicyEndpoint = (value: string) =>
    value.trim().replace(/\/+$/, "").toLowerCase();
  const normalizedRemoteEndpoint = normalizePolicyEndpoint(modelSettings.remote_profile.chat_endpoint);
  const normalizedAllowedEndpoints = enterprisePolicy.allowed_model_endpoints
    .map(normalizePolicyEndpoint)
    .filter(Boolean);
  const normalizedAllowedModels = enterprisePolicy.allowed_models
    .map((item) => item.trim())
    .filter(Boolean);
  const activeProviderPolicyBlock = useMemo(() => {
    if (modelSettings.active_provider === "ollama_local") {
      return null;
    }
    if (enterprisePolicy.egress_mode === "local_only") {
      return t("policyStatusLocalOnly");
    }
    if (
      normalizedAllowedEndpoints.length > 0 &&
      !normalizedAllowedEndpoints.includes(normalizedRemoteEndpoint)
    ) {
      return t("policyStatusEndpointBlocked");
    }
    if (
      normalizedAllowedModels.length > 0 &&
      [modelSettings.remote_profile.chat_model, modelSettings.remote_profile.graph_model, modelSettings.remote_profile.embed_model]
        .map((item) => item.trim())
        .some((item) => item && !normalizedAllowedModels.includes(item))
    ) {
      return t("policyStatusModelBlocked");
    }
    return null;
  }, [
    enterprisePolicy.egress_mode,
    modelSettings.active_provider,
    modelSettings.remote_profile.chat_model,
    modelSettings.remote_profile.embed_model,
    modelSettings.remote_profile.graph_model,
    normalizedAllowedEndpoints,
    normalizedAllowedModels,
    normalizedRemoteEndpoint,
    t
  ]);

  const tabMeta = useMemo(
    () => [
      {
        key: "basic" as const,
        label: t("basic"),
        icon: Cpu,
        keywords: [t("uiLanguage"), t("aiReplyLanguage"), t("watchRoot"), t("topK")]
      },
      {
        key: "models" as const,
        label: t("models"),
        icon: Settings,
        keywords: [t("modelProvider"), t("chatModel"), t("graphModel"), t("embedModel")]
      },
      {
        key: "memory" as const,
        label: t("memory"),
        icon: Brain,
        keywords: [
          t("conversationMemory"),
          t("autoMemoryWrite"),
          t("contextBudget"),
          t("memoryWriteSource"),
          "STM",
          "MTM",
          "LTM"
        ]
      },
      {
        key: "mcp" as const,
        label: t("mcp"),
        icon: Network,
        keywords: [t("mcpTransport"), t("mcpEndpoint"), t("mcpClientConfig"), "MCP", t("memory")]
      },
      {
        key: "advanced" as const,
        label: t("advanced"),
        icon: Database,
        keywords: [
          t("indexingMode"),
          t("resourceBudget"),
          t("triggerReindex"),
          t("pauseIndexing"),
          t("resumeIndexing")
        ]
      },
      {
        key: "personalization" as const,
        label: t("personalization"),
        icon: Palette,
        keywords: [t("fontPreset"), t("fontSize"), t("themeToggle")]
      },
      {
        key: "logs" as const,
        label: "日志",
        icon: ScrollText,
        keywords: ["日志", "log", "debug", "错误"]
      }
    ],
    [t]
  );

  const localFilteredTabs = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return tabMeta;
    return tabMeta.filter((tab) =>
      [tab.label, ...tab.keywords].some((item) => item.toLowerCase().includes(q))
    );
  }, [search, tabMeta]);

  useEffect(() => {
    const query = search.trim();
    if (!query) {
      setAiMatchedKeys(null);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      const candidates = tabMeta.map((tab) => ({
        key: tab.key,
        text: `${tab.label} ${tab.keywords.join(" ")}`
      }));
      void rankSettingsQuery({ query, candidates, lang: uiLang })
        .then((keys) => {
          if (cancelled) return;
          const valid = keys.filter((key): key is TabKey => tabMeta.some((tab) => tab.key === key));
          setAiMatchedKeys(valid.length > 0 ? valid : null);
        })
        .catch(() => {
          if (!cancelled) setAiMatchedKeys(null);
        });
    }, 280);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [search, tabMeta, uiLang]);

  const filteredTabs = useMemo(() => {
    if (!search.trim()) return tabMeta;
    if (aiMatchedKeys && aiMatchedKeys.length > 0) {
      const map = new Map(tabMeta.map((tab) => [tab.key, tab] as const));
      return aiMatchedKeys
        .map((key) => map.get(key))
        .filter((tab): tab is (typeof tabMeta)[number] => Boolean(tab));
    }
    return localFilteredTabs;
  }, [aiMatchedKeys, localFilteredTabs, search, tabMeta]);

  useEffect(() => {
    if (filteredTabs.length === 0) return;
    if (!filteredTabs.some((tab) => tab.key === activeTab)) {
      setActiveTab(filteredTabs[0].key);
    }
  }, [activeTab, filteredTabs]);

  useEffect(() => {
    const merged = providerModels.merged ?? [];
    const next: Record<ModelRole, boolean> = {
      chat_model: !merged.includes(activeProfile.chat_model),
      graph_model: !merged.includes(activeProfile.graph_model),
      embed_model: !merged.includes(activeProfile.embed_model)
    };
    setCustomMode((prev) => {
      if (
        prev.chat_model === next.chat_model &&
        prev.graph_model === next.graph_model &&
        prev.embed_model === next.embed_model
      ) {
        return prev;
      }
      return next;
    });
  }, [activeProfile.chat_model, activeProfile.embed_model, activeProfile.graph_model, providerModels.merged]);

  const fontPresetOptions = [
    { value: "system" as const, label: t("fontPresetSystem") },
    { value: "neo" as const, label: t("fontPresetNeo") },
    { value: "mono" as const, label: t("fontPresetMono") }
  ];
  const fontScaleOptions = [
    { value: "s" as const, label: t("fontSizeS") },
    { value: "m" as const, label: t("fontSizeM") },
    { value: "l" as const, label: t("fontSizeL") }
  ];

  const updateActiveProfile = (next: Partial<LocalModelProfileDto & RemoteModelProfileDto>) => {
    if (activeProvider === "ollama_local") {
      onModelSettingsChange({
        ...modelSettings,
        local_profile: { ...modelSettings.local_profile, ...next }
      });
      return;
    }
    onModelSettingsChange({
      ...modelSettings,
      remote_profile: { ...modelSettings.remote_profile, ...next }
    });
  };

  const onModelAction = async (key: ModelActionKey, action: () => Promise<void>) => {
    if (resetTimersRef.current[key]) {
      window.clearTimeout(resetTimersRef.current[key] as number);
      resetTimersRef.current[key] = null;
    }
    setActionState((prev) => ({
      ...prev,
      [key]: { phase: "running", tick: prev[key].tick + 1 }
    }));
    try {
      await action();
      setActionState((prev) => ({
        ...prev,
        [key]: { phase: "success", tick: prev[key].tick + 1 }
      }));
      resetTimersRef.current[key] = window.setTimeout(() => {
        setActionState((prev) => ({
          ...prev,
          [key]: { phase: "idle", tick: prev[key].tick + 1 }
        }));
      }, 2200);
    } catch {
      setActionState((prev) => ({
        ...prev,
        [key]: { phase: "error", tick: prev[key].tick + 1 }
      }));
      resetTimersRef.current[key] = window.setTimeout(() => {
        setActionState((prev) => ({
          ...prev,
          [key]: { phase: "idle", tick: prev[key].tick + 1 }
        }));
      }, 2600);
    }
  };

  const onIndexingAction = async (key: IndexingActionKey, action: () => Promise<void>) => {
    setIndexingAction((prev) => ({ key, phase: "running", tick: prev.tick + 1 }));
    try {
      await action();
      setIndexingAction((prev) => ({ key, phase: "success", tick: prev.tick + 1 }));
      window.setTimeout(() => {
        setIndexingAction((prev) => ({ key: prev.key, phase: "idle", tick: prev.tick + 1 }));
      }, 1800);
    } catch {
      setIndexingAction((prev) => ({ key, phase: "error", tick: prev.tick + 1 }));
      window.setTimeout(() => {
        setIndexingAction((prev) => ({ key: prev.key, phase: "idle", tick: prev.tick + 1 }));
      }, 2200);
    }
  };

  useEffect(() => {
    return () => {
      const timers = resetTimersRef.current;
      (Object.keys(timers) as ModelActionKey[]).forEach((key) => {
        if (timers[key]) {
          window.clearTimeout(timers[key] as number);
        }
      });
    };
  }, []);

  const buttonLabelByState = (key: ModelActionKey) => {
    const phase = actionState[key].phase;
    if (key === "probe") {
      if (phase === "running") return t("actionConnecting");
      if (phase === "success") return t("actionConnected");
      if (phase === "error") return t("actionConnectFailed");
      return t("testConnection");
    }
    if (key === "refresh") {
      if (phase === "running") return t("actionRefreshing");
      if (phase === "success") return t("actionRefreshed");
      if (phase === "error") return t("actionRefreshFailed");
      return t("refreshModels");
    }
    if (key === "save") {
      if (phase === "running") return t("actionSaving");
      if (phase === "success") return t("actionSaved");
      if (phase === "error") return t("actionSaveFailed");
      return t("saveModels");
    }
    if (phase === "running") return t("actionPulling");
    if (phase === "success") return t("actionPulled");
    if (phase === "error") return t("actionPullFailed");
    return t("pullMissingModels");
  };

  const buttonClassByState = (key: ModelActionKey) => {
    const phase = actionState[key].phase;
    if (phase === "success") {
      return "bg-[var(--accent-soft)] text-[var(--accent)] shadow-[0_0_8px_rgba(88,166,255,0.2)]";
    }
    if (phase === "error") {
      return "bg-red-500/15 text-red-300 shadow-[0_0_14px_rgba(239,68,68,0.3)]";
    }
    if (phase === "running") {
      return "bg-[var(--accent-soft)] text-[var(--accent)]";
    }
    return "bg-transparent text-[var(--text-primary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]";
  };
  const stableActionButtonClass =
    "inline-flex h-9 w-[170px] items-center justify-center gap-1.5 rounded-md px-3 text-sm whitespace-nowrap transition disabled:opacity-60";

  const onProviderSwitch = (provider: ModelProvider) => {
    onModelSettingsChange({
      ...modelSettings,
      active_provider: provider
    });
  };

  const indexingPhaseLabel = useMemo(() => {
    const normalized = indexingStatus?.phase?.toLowerCase() ?? "idle";
    if (normalized === "scanning") return t("indexingPhaseScanning");
    if (normalized === "embedding") return t("indexingPhaseEmbedding");
    if (normalized === "graphing") return t("indexingPhaseGraphing");
    return t("indexingPhaseIdle");
  }, [indexingStatus?.phase, t]);

  const indexingRebuildLabel = useMemo(() => {
    const normalized = indexingStatus?.rebuild_state?.toLowerCase() ?? "ready";
    if (normalized === "required") return t("indexingRebuildRequired");
    if (normalized === "rebuilding") return t("indexingRebuildInProgress");
    return t("indexingRebuildReady");
  }, [indexingStatus?.rebuild_state, t]);

  const lastScanLabel = useMemo(() => {
    const ts = indexingStatus?.last_scan_at;
    if (!ts) {
      return t("indexingNever");
    }
    try {
      return new Date(ts * 1000).toLocaleString(uiLang === "zh-CN" ? "zh-CN" : "en-US");
    } catch {
      return String(ts);
    }
  }, [indexingStatus?.last_scan_at, t, uiLang]);

  const etaLabel = useMemo(() => {
    const backlog = Math.max(0, indexingStatus?.graph_backlog ?? 0);
    if (backlog === 0) {
      return uiLang === "zh-CN" ? "≈ 0 分钟" : "~0 min";
    }
    const perChunkSec =
      resourceBudget === "fast" ? 0.35 : resourceBudget === "balanced" ? 0.8 : 1.4;
    const totalSec = Math.ceil(backlog * perChunkSec);
    if (totalSec < 60) {
      return uiLang === "zh-CN" ? `≈ ${totalSec} 秒` : `~${totalSec}s`;
    }
    const minutes = Math.ceil(totalSec / 60);
    return uiLang === "zh-CN" ? `≈ ${minutes} 分钟` : `~${minutes} min`;
  }, [indexingStatus?.graph_backlog, resourceBudget, uiLang]);

  const indexingButtonClass = (key: IndexingActionKey) => {
    if (indexingAction.key !== key) {
      return "bg-transparent text-[var(--text-primary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]";
    }
    if (indexingAction.phase === "success") {
      return "bg-[var(--accent-soft)] text-[var(--accent)] shadow-[0_0_8px_rgba(88,166,255,0.2)]";
    }
    if (indexingAction.phase === "error") {
      return "bg-red-500/15 text-red-300 shadow-[0_0_14px_rgba(239,68,68,0.3)]";
    }
    if (indexingAction.phase === "running") {
      return "bg-[var(--accent-soft)] text-[var(--accent)]";
    }
    return "bg-transparent text-[var(--text-primary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]";
  };

  return (
    <motion.aside
      initial={{ x: 140, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: 140, opacity: 0 }}
      transition={{ type: "spring", damping: 26, stiffness: 300 }}
      className="surface-lite-strong pointer-events-auto h-full w-[78%] overflow-hidden shadow-[-24px_0_44px_-26px_rgba(0,0,0,0.48),24px_0_44px_-26px_rgba(0,0,0,0.24)]"
      data-open={open}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="flex h-11 items-center justify-between px-4 shadow-[0_10px_18px_-16px_rgba(88,166,255,0.25)]">
        <AnimatedPressButton
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-1.5 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
          aria-label={t("back")}
          title={t("back")}
        >
          <ArrowRight className="h-4 w-4" />
          <span className="text-xs tracking-[0.1em] uppercase">{t("back")}</span>
        </AnimatedPressButton>
        <span className="text-xs tracking-[0.16em] text-[var(--text-secondary)] uppercase">
          {t("settingsTitle")}
        </span>
      </div>

      <div className="flex h-[calc(100%-44px)] min-h-0">
        <aside className="surface-lite h-full w-[28%] p-3 shadow-[10px_0_18px_-16px_rgba(88,166,255,0.28)]">
          <div className="mb-3 px-2 pt-1 text-xs tracking-[0.16em] text-[var(--text-secondary)] uppercase">
            {t("settings")}
          </div>
          <div className="relative mb-3">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t("settingsSearchPlaceholder")}
              className="h-9 w-full rounded-md border-none bg-transparent pl-8 pr-2 text-sm text-[var(--text-primary)] outline-none placeholder:text-[var(--text-muted)] focus:ring-0"
            />
          </div>
          {filteredTabs.length === 0 ? (
            <div className="rounded-lg border border-[var(--line-soft)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)] shadow-[0_8px_22px_-18px_rgba(88,166,255,0.24)]">
              {t("noSettingsMatch")}
            </div>
          ) : (
            <div className="space-y-1">
              {filteredTabs.map((tab) => {
                const Icon = tab.icon;
                const active = activeTab === tab.key;
                return (
                  <AnimatedPressButton
                    key={tab.key}
                    type="button"
                    onClick={() => setActiveTab(tab.key)}
                    className={`relative flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                      active ? "text-[var(--accent)]" : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                    }`}
                  >
                    {active ? (
                      <motion.span
                        layoutId="settings-tab-active-indicator"
                        className="absolute left-0 h-4 w-[2px] rounded bg-[var(--accent)]"
                        transition={{ type: "spring", stiffness: 420, damping: 34, mass: 0.62 }}
                      />
                    ) : null}
                    <Icon className="h-4 w-4" />
                    <span>{tab.label}</span>
                  </AnimatedPressButton>
                );
              })}
            </div>
          )}
        </aside>

        <section className="settings-scrollbar relative min-h-0 w-[72%] overflow-y-auto px-5 py-5">
          <AnimatePresence mode="wait">
            {activeTab === "basic" ? (
              <BasicTab
                t={t}
                uiLang={uiLang}
                aiLang={aiLang}
                onUiLangChange={onUiLangChange}
                onAiLangChange={onAiLangChange}
                retrieveTopK={retrieveTopK}
                onRetrieveTopKChange={onRetrieveTopKChange}
                watchRoot={watchRoot}
                isPickingWatchRoot={isPickingWatchRoot}
                onPickWatchRoot={onPickWatchRoot}
                autoSyncDaemon={autoSyncDaemon}
                onAutoSyncDaemonChange={setAutoSyncDaemon}
                graphRagInfer={graphRagInfer}
                onGraphRagInferChange={setGraphRagInfer}
              />
            ) : null}

                        {activeTab === "models" ? (
              <ModelsTab
                t={t}
                modelSettings={modelSettings}
                enterprisePolicy={enterprisePolicy}
                modelAvailability={modelAvailability}
                providerModels={providerModels}
                modelBusy={modelBusy}
                enterpriseBusy={enterpriseBusy}
                customMode={customMode}
                setCustomMode={setCustomMode}
                activeProviderPolicyBlock={activeProviderPolicyBlock}
                onProviderSwitch={onProviderSwitch}
                onEnterprisePolicyChange={onEnterprisePolicyChange}
                onSaveEnterprisePolicy={onSaveEnterprisePolicy}
                updateActiveProfile={updateActiveProfile}
                onModelAction={onModelAction}
                onProbeModelProvider={onProbeModelProvider}
                onRefreshProviderModels={onRefreshProviderModels}
                onSaveModelSettings={onSaveModelSettings}
                onPullModel={onPullModel}
                onPickLocalModelsRoot={onPickLocalModelsRoot}
                onClearLocalModelsRoot={onClearLocalModelsRoot}
                actionState={actionState}
                buttonClassByState={buttonClassByState}
                buttonLabelByState={buttonLabelByState}
                stableActionButtonClass={stableActionButtonClass}
              />
            ) : null}

                        {activeTab === "advanced" ? (
              <AdvancedTab
                t={t}
                indexingMode={indexingMode}
                onIndexingModeChange={onIndexingModeChange}
                resourceBudget={resourceBudget}
                onResourceBudgetChange={onResourceBudgetChange}
                scheduleStart={scheduleStart}
                scheduleEnd={scheduleEnd}
                onScheduleStartChange={onScheduleStartChange}
                onScheduleEndChange={onScheduleEndChange}
                indexingStatus={indexingStatus}
                indexingBusy={indexingBusy}
                indexingPhaseLabel={indexingPhaseLabel}
                indexingRebuildLabel={indexingRebuildLabel}
                lastScanLabel={lastScanLabel}
                etaLabel={etaLabel}
                stableActionButtonClass={stableActionButtonClass}
                indexingButtonClass={indexingButtonClass}
                indexingAction={indexingAction}
                onIndexingAction={onIndexingAction}
                onSaveIndexingConfig={onSaveIndexingConfig}
                onTriggerReindex={onTriggerReindex}
                onPauseIndexing={onPauseIndexing}
                onResumeIndexing={onResumeIndexing}
              />
            ) : null}

                        {activeTab === "mcp" ? (
              <McpTab
                t={t}
                mcpSettings={mcpSettings}
                mcpStatus={mcpStatus}
                mcpBusy={mcpBusy}
                mcpMessage={mcpMessage}
                onMcpSettingsChange={onMcpSettingsChange}
                onSaveMcpSettings={onSaveMcpSettings}
                onCopyMcpClientConfig={onCopyMcpClientConfig}
              />
            ) : null}

                        {activeTab === "memory" ? (
              <MemoryTab
                t={t}
                memorySettings={memorySettings}
                memoryBusy={memoryBusy}
                memoryMessage={memoryMessage}
                onMemorySettingsChange={onMemorySettingsChange}
                onSaveMemorySettings={onSaveMemorySettings}
              />
            ) : null}

                        {activeTab === "personalization" ? (
              <PersonalizationTab
                t={t}
                fontPreset={fontPreset}
                onFontPresetChange={onFontPresetChange}
                fontScale={fontScale}
                onFontScaleChange={onFontScaleChange}
                themeMode={themeMode}
                onThemeModeChange={onThemeModeChange}
                fontPresetOptions={fontPresetOptions}
                fontScaleOptions={fontScaleOptions}
              />
            ) : null}

            {activeTab === "logs" ? <LogsTab /> : null}
          </AnimatePresence>
        </section>
      </div>
    </motion.aside>
  );
}
