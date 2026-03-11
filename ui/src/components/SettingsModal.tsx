import { AnimatePresence, motion } from "framer-motion";
import { ArrowRight, Cpu, Database, LoaderCircle, Palette, Search, Settings } from "lucide-react";
import { ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "../i18n";
import { useI18n } from "../i18n";
import {
  AnimatedPanel,
  AnimatedPressButton,
  fadeSlideUpVariants,
  staggerContainerVariants
} from "./MotionKit";
import { CyberInput, CyberToggle } from "./UI";

export type FontPreset = "system" | "neo" | "mono";
export type FontScale = "s" | "m" | "l";
export type ThemeMode = "dark" | "light";
export type ModelProvider = "ollama_local" | "openai_compatible";
export type IndexingMode = "continuous" | "manual" | "scheduled";
export type ResourceBudget = "low" | "balanced" | "fast";
type ModelRole = "chat_model" | "graph_model" | "embed_model";

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

type SettingsModalProps = {
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

type TabKey = "basic" | "models" | "advanced" | "personalization";

function LanguageSwitch({
  value,
  onChange
}: {
  value: Language;
  onChange: (lang: Language) => void;
}) {
  return (
    <div className="inline-flex items-center gap-2 rounded-lg bg-transparent p-1">
      <AnimatedPressButton
        type="button"
        onClick={() => onChange("zh-CN")}
        className={`rounded-md px-2.5 py-1 text-sm transition ${
          value === "zh-CN"
            ? "bg-transparent text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
        }`}
      >
        CN
      </AnimatedPressButton>
      <AnimatedPressButton
        type="button"
        onClick={() => onChange("en-US")}
        className={`rounded-md px-2.5 py-1 text-sm transition ${
          value === "en-US"
            ? "bg-transparent text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
        }`}
      >
        EN
      </AnimatedPressButton>
    </div>
  );
}

function SelectionChips<T extends string>({
  value,
  onChange,
  options
}: {
  value: T;
  onChange: (value: T) => void;
  options: Array<{ value: T; label: string }>;
}) {
  return (
    <div className="inline-flex flex-wrap items-center gap-2">
      {options.map((option) => (
        <AnimatedPressButton
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={`rounded-md px-3 py-1.5 text-sm transition ${
            value === option.value
              ? "bg-transparent text-[var(--accent)]"
              : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
          }`}
        >
          {option.label}
        </AnimatedPressButton>
      ))}
    </div>
  );
}

function SettingCard({
  title,
  description,
  children
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <AnimatedPanel className="glass-panel-infer flex items-center justify-between gap-4 rounded-lg px-3 py-3">
      <div className="min-w-0">
        <div className="text-sm text-[var(--text-primary)]">{title}</div>
        {description ? <div className="mt-1 text-xs text-[var(--text-secondary)]">{description}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </AnimatedPanel>
  );
}

const CUSTOM_VALUE = "__custom__";

function ModelRoleSelector({
  label,
  value,
  options,
  customMode,
  onToggleCustom,
  onChange
}: {
  label: string;
  value: string;
  options: string[];
  customMode: boolean;
  onToggleCustom: () => void;
  onChange: (value: string) => void;
}) {
  const { t } = useI18n();
  const hasValue = options.includes(value);
  const selectValue = customMode || !hasValue ? CUSTOM_VALUE : value;
  return (
    <AnimatedPanel className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
      <div className="text-sm text-[var(--text-primary)]">{label}</div>
      <div className="flex items-center gap-2">
        <select
          value={selectValue}
          onChange={(event) => {
            const next = event.target.value;
            if (next === CUSTOM_VALUE) {
              onToggleCustom();
            } else {
              onChange(next);
            }
          }}
          className="h-9 min-w-0 flex-1 rounded-lg border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
        >
          {options.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
          <option value={CUSTOM_VALUE}>{t("modelUseCustom")}</option>
        </select>
        <AnimatedPressButton
          type="button"
          onClick={onToggleCustom}
          className={`rounded-md px-3 py-1.5 text-sm transition ${
            customMode
              ? "bg-[var(--accent-soft)] text-[var(--accent)]"
              : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
          }`}
        >
          {t("modelUseCustom")}
        </AnimatedPressButton>
      </div>
      {customMode ? (
        <CyberInput value={value} onChange={onChange} placeholder={t("modelCustomPlaceholder")} />
      ) : null}
    </AnimatedPanel>
  );
}

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
  onResumeIndexing
}: SettingsModalProps) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const [search, setSearch] = useState("");
  const [aiMatchedKeys, setAiMatchedKeys] = useState<TabKey[] | null>(null);
  const [autoSyncDaemon, setAutoSyncDaemon] = useState(true);
  const [graphRagInfer, setGraphRagInfer] = useState(true);
  type ActionKey = "probe" | "refresh" | "save" | "pull";
  type ActionPhase = "idle" | "running" | "success" | "error";
  const [actionState, setActionState] = useState<Record<ActionKey, { phase: ActionPhase; tick: number }>>({
    probe: { phase: "idle", tick: 0 },
    refresh: { phase: "idle", tick: 0 },
    save: { phase: "idle", tick: 0 },
    pull: { phase: "idle", tick: 0 }
  });
  const resetTimersRef = useRef<Record<ActionKey, number | null>>({
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
  type IndexingActionKey = "saveIndexing" | "triggerReindex" | "pauseResume";
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
  const normalizedRemoteEndpoint = normalizePolicyEndpoint(modelSettings.remote_profile.endpoint);
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
      void invoke<string[]>("rank_settings_query", { query, candidates, lang: uiLang })
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

  const onModelAction = async (key: ActionKey, action: () => Promise<void>) => {
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
      (Object.keys(timers) as ActionKey[]).forEach((key) => {
        if (timers[key]) {
          window.clearTimeout(timers[key] as number);
        }
      });
    };
  }, []);

  const buttonLabelByState = (key: ActionKey) => {
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

  const buttonClassByState = (key: ActionKey) => {
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
      className="pointer-events-auto h-full w-[78%] overflow-hidden bg-[var(--bg-surface-1)] shadow-[-24px_0_44px_-26px_rgba(0,0,0,0.48),24px_0_44px_-26px_rgba(0,0,0,0.24)] backdrop-blur-xl"
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
        <aside className="h-full w-[28%] bg-[var(--bg-surface-2)] p-3 shadow-[10px_0_18px_-16px_rgba(88,166,255,0.28)]">
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
              <motion.div
                key="settings-tab-basic"
                variants={fadeSlideUpVariants}
                initial="hidden"
                animate="show"
                exit="exit"
                className="pt-2"
              >
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("basic")}
              </h3>
              <motion.div
                variants={staggerContainerVariants}
                initial="hidden"
                animate="show"
                className="space-y-4 pb-2"
              >
                <SettingCard title={t("uiLanguage")}>
                  <LanguageSwitch value={uiLang} onChange={onUiLangChange} />
                </SettingCard>
                <SettingCard title={t("aiReplyLanguage")}>
                  <LanguageSwitch value={aiLang} onChange={onAiLangChange} />
                </SettingCard>
                <SettingCard title={t("topK")} description={t("topKDesc")}>
                  <SelectionChips
                    value={String(retrieveTopK)}
                    onChange={(value) => onRetrieveTopKChange(Number(value))}
                    options={[
                      { value: "5", label: "5" },
                      { value: "10", label: "10" },
                      { value: "20", label: "20" },
                      { value: "30", label: "30" },
                      { value: "50", label: "50" }
                    ]}
                  />
                </SettingCard>
                <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-sm text-[var(--text-primary)]">{t("watchRoot")}</div>
                      <div className="mt-1 truncate font-mono text-xs text-[var(--text-secondary)]" title={watchRoot}>
                        {watchRoot || "-"}
                      </div>
                    </div>
                    <AnimatedPressButton
                      type="button"
                      onClick={onPickWatchRoot}
                      disabled={isPickingWatchRoot}
                      className="inline-flex items-center rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {isPickingWatchRoot ? t("watchRootPicking") : t("watchRootPick")}
                    </AnimatedPressButton>
                  </div>
                  <div className="mt-2 text-xs text-[var(--text-secondary)]">{t("watchRootRestartHint")}</div>
                </AnimatedPanel>
                <SettingCard title={t("autoSyncDaemon")} description={t("autoSyncDaemonDesc")}>
                  <CyberToggle
                    checked={autoSyncDaemon}
                    onChange={setAutoSyncDaemon}
                    ariaLabel={t("autoSyncDaemon")}
                  />
                </SettingCard>
                <SettingCard title={t("graphRagInfer")} description={t("graphRagInferDesc")}>
                  <CyberToggle
                    checked={graphRagInfer}
                    onChange={setGraphRagInfer}
                    ariaLabel={t("graphRagInfer")}
                  />
                </SettingCard>
              </motion.div>
            </motion.div>
          ) : null}

            {activeTab === "models" ? (
              <motion.div
                key="settings-tab-models"
                variants={fadeSlideUpVariants}
                initial="hidden"
                animate="show"
                exit="exit"
                className="pt-2"
              >
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("models")}
              </h3>
              <motion.div
                variants={staggerContainerVariants}
                initial="hidden"
                animate="show"
                className="space-y-3"
              >
                <SettingCard title={t("modelProvider")}>
                  <SelectionChips
                    value={modelSettings.active_provider}
                    onChange={onProviderSwitch}
                    options={[
                      { value: "ollama_local", label: t("providerOllama") },
                      { value: "openai_compatible", label: t("providerOpenAI") }
                    ]}
                  />
                </SettingCard>

                <AnimatedPanel className="glass-panel-infer space-y-3 rounded-lg px-3 py-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-sm text-[var(--text-primary)]">{t("policyTitle")}</div>
                      <div className="mt-1 text-xs text-[var(--text-secondary)]">
                        {t("policyDesc")}
                      </div>
                    </div>
                    <AnimatedPressButton
                      type="button"
                      onClick={() => void onSaveEnterprisePolicy()}
                      disabled={enterpriseBusy}
                      className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
                    >
                      {t("policySave")}
                    </AnimatedPressButton>
                  </div>
                  <SelectionChips
                    value={enterprisePolicy.egress_mode}
                    onChange={(value) =>
                      onEnterprisePolicyChange({
                        ...enterprisePolicy,
                        egress_mode: value
                      })
                    }
                    options={[
                      { value: "local_only", label: t("policyModeLocalOnly") },
                      { value: "allowlist", label: t("policyModeAllowlist") }
                    ]}
                  />
                  <div className="grid gap-3 md:grid-cols-2">
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">
                        {t("policyAllowedEndpoints")}
                      </div>
                      <textarea
                        value={enterprisePolicy.allowed_model_endpoints.join("\n")}
                        onChange={(event) =>
                          onEnterprisePolicyChange({
                            ...enterprisePolicy,
                            allowed_model_endpoints: event.target.value
                              .split(/\r?\n/)
                              .map((item) => item.trim())
                              .filter(Boolean)
                          })
                        }
                        className="min-h-[96px] w-full rounded-lg border border-transparent bg-transparent px-3 py-2 text-xs text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                        placeholder="https://models.company.local/v1"
                      />
                    </div>
                    <div>
                      <div className="mb-1 text-xs text-[var(--text-secondary)]">
                        {t("policyAllowedModels")}
                      </div>
                      <textarea
                        value={enterprisePolicy.allowed_models.join("\n")}
                        onChange={(event) =>
                          onEnterprisePolicyChange({
                            ...enterprisePolicy,
                            allowed_models: event.target.value
                              .split(/\r?\n/)
                              .map((item) => item.trim())
                              .filter(Boolean)
                          })
                        }
                        className="min-h-[96px] w-full rounded-lg border border-transparent bg-transparent px-3 py-2 text-xs text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                        placeholder="nomic-embed-text:latest"
                      />
                    </div>
                  </div>
                  <div className="grid gap-2 text-xs text-[var(--text-secondary)] md:grid-cols-2">
                    <div>
                      {t("policyCurrentMode", {
                        mode:
                          enterprisePolicy.egress_mode === "local_only"
                            ? t("policyModeLocalOnly")
                            : t("policyModeAllowlist")
                      })}
                    </div>
                    <div>{t("policyEndpointCount", { count: enterprisePolicy.allowed_model_endpoints.length })}</div>
                    <div>{t("policyModelCount", { count: enterprisePolicy.allowed_models.length })}</div>
                    <div className={activeProviderPolicyBlock ? "text-amber-300" : "text-[var(--text-secondary)]"}>
                      {activeProviderPolicyBlock ?? t("policyStatusAllowed")}
                    </div>
                  </div>
                </AnimatedPanel>

                <SettingCard title={t("modelEndpoint")}>
                  <div className="w-[320px]">
                    <CyberInput
                      value={activeProfile.endpoint}
                      onChange={(value) => updateActiveProfile({ endpoint: value })}
                      placeholder={
                        activeProvider === "ollama_local"
                          ? "http://localhost:11434"
                          : "https://api.openai.com"
                      }
                    />
                  </div>
                </SettingCard>

                {activeProvider === "openai_compatible" ? (
                  <SettingCard title={t("modelApiKey")}>
                    <div className="w-[320px]">
                      <CyberInput
                        value={modelSettings.remote_profile.api_key ?? ""}
                        onChange={(value) => updateActiveProfile({ api_key: value })}
                        placeholder="sk-..."
                      />
                    </div>
                  </SettingCard>
                ) : null}

                {activeProvider === "ollama_local" ? (
                  <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-sm text-[var(--text-primary)]">{t("modelLocalRoot")}</div>
                        <div className="mt-1 truncate font-mono text-xs text-[var(--text-secondary)]">
                          {modelSettings.local_profile.models_root || "-"}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <AnimatedPressButton
                          type="button"
                          onClick={() => void onModelAction("refresh", onPickLocalModelsRoot)}
                          disabled={modelBusy}
                          className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
                        >
                          {t("modelLocalRootPick")}
                        </AnimatedPressButton>
                        <AnimatedPressButton
                          type="button"
                          onClick={onClearLocalModelsRoot}
                          disabled={modelBusy}
                          className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
                        >
                          {t("modelLocalRootClear")}
                        </AnimatedPressButton>
                      </div>
                    </div>
                  </AnimatedPanel>
                ) : null}

                <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                  <div className="mb-2 text-sm text-[var(--text-primary)]">{t("modelMergedCandidates")}</div>
                  <div className="flex flex-wrap items-center gap-3 text-xs text-[var(--text-secondary)]">
                    <span>
                      {t("modelFromFolder")}: {providerModels.from_folder.length}
                    </span>
                    <span>
                      {t("modelFromService")}: {providerModels.from_service.length}
                    </span>
                    <span>
                      {t("modelMergedCandidates")}: {providerModels.merged.length}
                    </span>
                  </div>
                  {providerModels.merged.length === 0 ? (
                    <div className="mt-2 text-xs text-[var(--text-secondary)]">{t("modelNoCandidates")}</div>
                  ) : null}
                </AnimatedPanel>

                <ModelRoleSelector
                  label={t("chatModel")}
                  value={activeProfile.chat_model}
                  options={providerModels.merged}
                  customMode={customMode.chat_model}
                  onToggleCustom={() =>
                    setCustomMode((prev) => ({ ...prev, chat_model: !prev.chat_model }))
                  }
                  onChange={(value) => updateActiveProfile({ chat_model: value })}
                />
                <ModelRoleSelector
                  label={t("graphModel")}
                  value={activeProfile.graph_model}
                  options={providerModels.merged}
                  customMode={customMode.graph_model}
                  onToggleCustom={() =>
                    setCustomMode((prev) => ({ ...prev, graph_model: !prev.graph_model }))
                  }
                  onChange={(value) => updateActiveProfile({ graph_model: value })}
                />
                <ModelRoleSelector
                  label={t("embedModel")}
                  value={activeProfile.embed_model}
                  options={providerModels.merged}
                  customMode={customMode.embed_model}
                  onToggleCustom={() =>
                    setCustomMode((prev) => ({ ...prev, embed_model: !prev.embed_model }))
                  }
                  onChange={(value) => updateActiveProfile({ embed_model: value })}
                />

                <AnimatedPanel className="glass-panel-infer space-y-2 rounded-md px-3 py-2 text-xs text-[var(--text-secondary)]">
                  <div className="text-[var(--text-primary)]">{t("modelStatusTitle")}</div>
                  <div className="flex flex-wrap gap-2">
                    <motion.button
                      key="probe"
                      type="button"
                      onClick={() => void onModelAction("probe", onProbeModelProvider)}
                      disabled={modelBusy}
                      className={`${stableActionButtonClass} ${buttonClassByState("probe")}`}
                      animate={
                        actionState.probe.phase === "success"
                          ? { scale: [1, 1.015, 1] }
                          : { scale: 1 }
                      }
                      transition={{ duration: 0.35 }}
                    >
                      {actionState.probe.phase === "running" ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : null}
                      {buttonLabelByState("probe")}
                    </motion.button>
                    <motion.button
                      key="refresh"
                      type="button"
                      onClick={() => void onModelAction("refresh", onRefreshProviderModels)}
                      disabled={modelBusy}
                      className={`${stableActionButtonClass} ${buttonClassByState("refresh")}`}
                      animate={
                        actionState.refresh.phase === "success"
                          ? { scale: [1, 1.015, 1] }
                          : { scale: 1 }
                      }
                      transition={{ duration: 0.35 }}
                    >
                      {actionState.refresh.phase === "running" ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : null}
                      {buttonLabelByState("refresh")}
                    </motion.button>
                    <motion.button
                      key="save"
                      type="button"
                      onClick={() => void onModelAction("save", onSaveModelSettings)}
                      disabled={modelBusy}
                      className={`${stableActionButtonClass} ${buttonClassByState("save")}`}
                      animate={
                        actionState.save.phase === "success"
                          ? { scale: [1, 1.015, 1] }
                          : { scale: 1 }
                      }
                      transition={{ duration: 0.35 }}
                    >
                      {actionState.save.phase === "running" ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : null}
                      {buttonLabelByState("save")}
                    </motion.button>
                    {activeProvider === "ollama_local" ? (
                      <motion.button
                        key="pull"
                        type="button"
                        onClick={() => {
                          const candidates = modelAvailability?.missing_roles ?? [];
                          if (candidates.includes("embed")) {
                            void onModelAction("pull", () => onPullModel(activeProfile.embed_model));
                          } else if (candidates.includes("chat")) {
                            void onModelAction("pull", () => onPullModel(activeProfile.chat_model));
                          } else if (candidates.includes("graph")) {
                            void onModelAction("pull", () => onPullModel(activeProfile.graph_model));
                          }
                        }}
                        disabled={modelBusy || !modelAvailability?.missing_roles?.length}
                        className={`${stableActionButtonClass} ${buttonClassByState("pull")}`}
                        animate={
                          actionState.pull.phase === "success"
                            ? { scale: [1, 1.015, 1] }
                            : { scale: 1 }
                        }
                        transition={{ duration: 0.35 }}
                      >
                        {actionState.pull.phase === "running" ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : null}
                        {buttonLabelByState("pull")}
                      </motion.button>
                    ) : null}
                  </div>
                  {modelAvailability ? (
                    <>
                      <div>
                        {modelAvailability.reachable ? t("modelStatusReachable") : t("modelStatusUnreachable")}
                      </div>
                      <div>
                        {modelAvailability.missing_roles.length > 0
                          ? t("modelStatusMissing", {
                              roles: modelAvailability.missing_roles.join(", ")
                            })
                          : t("modelStatusReady")}
                      </div>
                      {modelAvailability.checked_provider ? (
                        <div>
                          {t("modelStatusProvider", {
                            provider:
                              modelAvailability.checked_provider === "ollama_local"
                                ? t("providerOllama")
                                : t("providerOpenAI")
                          })}
                        </div>
                      ) : null}
                      {modelAvailability.errors.map((item, idx) => (
                        <div key={`${item.code}-${idx}`} className="text-red-300">
                          {item.code}: {item.message}
                        </div>
                      ))}
                    </>
                  ) : null}
                </AnimatedPanel>
              </motion.div>
            </motion.div>
          ) : null}

            {activeTab === "advanced" ? (
              <motion.div
                key="settings-tab-advanced"
                variants={fadeSlideUpVariants}
                initial="hidden"
                animate="show"
                exit="exit"
                className="pt-2"
              >
                <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                  {t("advanced")}
                </h3>
                <motion.div
                  variants={staggerContainerVariants}
                  initial="hidden"
                  animate="show"
                  className="space-y-4"
                >
                  <SettingCard title={t("indexingMode")} description={t("indexingModeDesc")}>
                    <SelectionChips
                      value={indexingMode}
                      onChange={onIndexingModeChange}
                      options={[
                        { value: "continuous", label: t("indexingModeContinuous") },
                        { value: "manual", label: t("indexingModeManual") },
                        { value: "scheduled", label: t("indexingModeScheduled") }
                      ]}
                    />
                  </SettingCard>

                  <SettingCard title={t("resourceBudget")} description={t("resourceBudgetDesc")}>
                    <SelectionChips
                      value={resourceBudget}
                      onChange={onResourceBudgetChange}
                      options={[
                        { value: "low", label: t("resourceBudgetLow") },
                        { value: "balanced", label: t("resourceBudgetBalanced") },
                        { value: "fast", label: t("resourceBudgetFast") }
                      ]}
                    />
                  </SettingCard>

                  {indexingMode === "scheduled" ? (
                    <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                      <div className="mb-2 text-sm text-[var(--text-primary)]">{t("scheduleWindow")}</div>
                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <div className="mb-1 text-xs text-[var(--text-secondary)]">
                            {t("scheduleWindowStart")}
                          </div>
                          <input
                            type="time"
                            value={scheduleStart}
                            onChange={(event) => onScheduleStartChange(event.target.value)}
                            className="h-9 w-full rounded-md border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                          />
                        </div>
                        <div>
                          <div className="mb-1 text-xs text-[var(--text-secondary)]">
                            {t("scheduleWindowEnd")}
                          </div>
                          <input
                            type="time"
                            value={scheduleEnd}
                            onChange={(event) => onScheduleEndChange(event.target.value)}
                            className="h-9 w-full rounded-md border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                          />
                        </div>
                      </div>
                    </AnimatedPanel>
                  ) : null}

                  <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                    <div className="mb-2 text-sm text-[var(--text-primary)]">{t("indexingStatusTitle")}</div>
                    <div className="grid grid-cols-2 gap-2 text-xs text-[var(--text-secondary)]">
                      <div>{t("indexingPhase")}</div>
                      <div className="text-[var(--text-primary)]">{indexingPhaseLabel}</div>
                      <div>{t("indexingDocs")}</div>
                      <div className="text-[var(--text-primary)]">{indexingStatus?.indexed_docs ?? 0}</div>
                      <div>{t("indexingChunks")}</div>
                      <div className="text-[var(--text-primary)]">{indexingStatus?.indexed_chunks ?? 0}</div>
                      <div>{t("indexingGraphedChunks")}</div>
                      <div className="text-[var(--text-primary)]">{indexingStatus?.graphed_chunks ?? 0}</div>
                      <div>{t("indexingBacklog")}</div>
                      <div className="text-[var(--text-primary)]">{indexingStatus?.graph_backlog ?? 0}</div>
                      <div>{t("indexingEta")}</div>
                      <div className="text-[var(--text-primary)]">{etaLabel}</div>
                      <div>{t("indexingLastScan")}</div>
                      <div className="text-[var(--text-primary)]">{lastScanLabel}</div>
                      <div>{t("indexingLastError")}</div>
                      <div className="text-[var(--text-primary)]">
                        {indexingStatus?.last_error?.trim() || t("indexingNoError")}
                      </div>
                      <div>{t("indexingRebuildState")}</div>
                      <div className="text-[var(--text-primary)]">{indexingRebuildLabel}</div>
                      <div>{t("indexingRebuildReason")}</div>
                      <div className="text-[var(--text-primary)]">
                        {indexingStatus?.rebuild_reason?.trim() || t("indexingNoError")}
                      </div>
                      <div>{t("indexingIndexVersion")}</div>
                      <div className="text-[var(--text-primary)]">
                        {indexingStatus?.index_format_version ?? 0}
                      </div>
                      <div>{t("indexingParserVersion")}</div>
                      <div className="text-[var(--text-primary)]">
                        {indexingStatus?.parser_format_version ?? 0}
                      </div>
                    </div>
                  </AnimatedPanel>

                  <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
                    <div className="flex flex-wrap gap-2">
                      <motion.button
                        key="save-indexing"
                        type="button"
                        disabled={indexingBusy}
                        onClick={() => void onIndexingAction("saveIndexing", onSaveIndexingConfig)}
                        className={`${stableActionButtonClass} ${indexingButtonClass("saveIndexing")}`}
                      >
                        {indexingAction.phase === "running" && indexingAction.key === "saveIndexing" ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : null}
                        {t("saveIndexingConfig")}
                      </motion.button>
                      <motion.button
                        key="trigger-indexing"
                        type="button"
                        disabled={indexingBusy}
                        onClick={() => void onIndexingAction("triggerReindex", onTriggerReindex)}
                        className={`${stableActionButtonClass} ${indexingButtonClass("triggerReindex")}`}
                      >
                        {indexingAction.phase === "running" && indexingAction.key === "triggerReindex" ? (
                          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        ) : null}
                        {t("triggerReindex")}
                      </motion.button>
                      {indexingStatus?.paused ? (
                        <motion.button
                          key="resume-indexing"
                          type="button"
                          disabled={indexingBusy}
                          onClick={() => void onIndexingAction("pauseResume", onResumeIndexing)}
                          className={`${stableActionButtonClass} ${indexingButtonClass("pauseResume")}`}
                        >
                          {indexingAction.phase === "running" && indexingAction.key === "pauseResume" ? (
                            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                          ) : null}
                          {t("resumeIndexing")}
                        </motion.button>
                      ) : (
                        <motion.button
                          key="pause-indexing"
                          type="button"
                          disabled={indexingBusy}
                          onClick={() => void onIndexingAction("pauseResume", onPauseIndexing)}
                          className={`${stableActionButtonClass} ${indexingButtonClass("pauseResume")}`}
                        >
                          {indexingAction.phase === "running" && indexingAction.key === "pauseResume" ? (
                            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                          ) : null}
                          {t("pauseIndexing")}
                        </motion.button>
                      )}
                    </div>
                  </AnimatedPanel>
                </motion.div>
              </motion.div>
            ) : null}

            {activeTab === "personalization" ? (
              <motion.div
                key="settings-tab-personalization"
                variants={fadeSlideUpVariants}
                initial="hidden"
                animate="show"
                exit="exit"
                className="pt-2"
              >
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("personalization")}
              </h3>
              <motion.div
                variants={staggerContainerVariants}
                initial="hidden"
                animate="show"
                className="space-y-4"
              >
                <SettingCard title={t("fontPreset")}>
                  <SelectionChips
                    value={fontPreset}
                    onChange={onFontPresetChange}
                    options={fontPresetOptions}
                  />
                </SettingCard>
                <SettingCard title={t("fontSize")}>
                  <SelectionChips
                    value={fontScale}
                    onChange={onFontScaleChange}
                    options={fontScaleOptions}
                  />
                </SettingCard>
                <SettingCard title={t("themeToggle")} description={t("themeToggleDesc")}>
                  <SelectionChips
                    value={themeMode}
                    onChange={onThemeModeChange}
                    options={[
                      { value: "dark", label: t("themeModeDark") },
                      { value: "light", label: t("themeModeLight") }
                    ]}
                  />
                </SettingCard>
              </motion.div>
            </motion.div>
          ) : null}
          </AnimatePresence>
        </section>
      </div>
    </motion.aside>
  );
}
