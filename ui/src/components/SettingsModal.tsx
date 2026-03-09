import { motion } from "framer-motion";
import { ArrowRight, Cpu, Database, LoaderCircle, Palette, Search, Settings } from "lucide-react";
import { ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "../i18n";
import { useI18n } from "../i18n";
import { CyberInput, CyberToggle } from "./UI";

export type FontPreset = "system" | "neo" | "mono";
export type FontScale = "s" | "m" | "l";
export type ThemeMode = "dark" | "light";
export type ModelProvider = "ollama_local" | "openai_compatible";
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

export type ProviderModelsDto = {
  from_folder: string[];
  from_service: string[];
  merged: string[];
};

export type ModelAvailabilityDto = {
  reachable: boolean;
  models: string[];
  missing_roles: string[];
  errors: Array<{ code: string; message: string }>;
  checked_provider?: string | null;
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
  modelAvailability: ModelAvailabilityDto | null;
  providerModels: ProviderModelsDto;
  modelBusy: boolean;
  onModelSettingsChange: (next: ModelSettingsDto) => void;
  onSaveModelSettings: () => Promise<void>;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
  onPullModel: (model: string) => Promise<void>;
  onPickLocalModelsRoot: () => Promise<void>;
  onClearLocalModelsRoot: () => void;
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
    <div className="inline-flex items-center rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] p-1">
      <button
        type="button"
        onClick={() => onChange("zh-CN")}
        className={`rounded-md px-2 py-1 text-xs transition ${
          value === "zh-CN"
            ? "bg-[var(--accent-soft)] text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        }`}
      >
        CN
      </button>
      <button
        type="button"
        onClick={() => onChange("en-US")}
        className={`rounded-md px-2 py-1 text-xs transition ${
          value === "en-US"
            ? "bg-[var(--accent-soft)] text-[var(--accent)]"
            : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        }`}
      >
        EN
      </button>
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
        <button
          key={option.value}
          type="button"
          onClick={() => onChange(option.value)}
          className={`rounded-md border px-2.5 py-1 text-xs transition ${
            value === option.value
              ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              : "border-[var(--border-strong)] bg-[var(--bg-surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
        >
          {option.label}
        </button>
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
    <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-3">
      <div className="min-w-0">
        <div className="text-sm text-[var(--text-primary)]">{title}</div>
        {description ? <div className="mt-1 text-xs text-[var(--text-secondary)]">{description}</div> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
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
    <div className="space-y-2 rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-3">
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
          className="h-9 min-w-0 flex-1 rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-2 text-sm text-[var(--text-primary)] outline-none focus:ring-1 focus:ring-[var(--accent)]"
        >
          {options.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
          <option value={CUSTOM_VALUE}>{t("modelUseCustom")}</option>
        </select>
        <button
          type="button"
          onClick={onToggleCustom}
          className={`rounded-md border px-2.5 py-1 text-xs transition ${
            customMode
              ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              : "border-[var(--border-strong)] bg-[var(--bg-surface-2)] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
        >
          {t("modelUseCustom")}
        </button>
      </div>
      {customMode ? (
        <CyberInput value={value} onChange={onChange} placeholder={t("modelCustomPlaceholder")} />
      ) : null}
    </div>
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
  modelAvailability,
  providerModels,
  modelBusy,
  onModelSettingsChange,
  onSaveModelSettings,
  onProbeModelProvider,
  onRefreshProviderModels,
  onPullModel,
  onPickLocalModelsRoot,
  onClearLocalModelsRoot
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

  const activeProvider = modelSettings.active_provider;
  const activeProfile =
    activeProvider === "ollama_local" ? modelSettings.local_profile : modelSettings.remote_profile;

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
        keywords: ["runtime diagnostics"]
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

  const buttonClassByState = (key: ActionKey, variant: "normal" | "primary" = "normal") => {
    const phase = actionState[key].phase;
    if (phase === "success") {
      return "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] shadow-[0_0_16px_rgba(88,166,255,0.45)]";
    }
    if (phase === "error") {
      return "border-red-500/60 bg-red-500/15 text-red-300 shadow-[0_0_14px_rgba(239,68,68,0.3)]";
    }
    if (phase === "running") {
      return "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]";
    }
    if (variant === "primary") {
      return "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]";
    }
    return "border-[var(--border-strong)] bg-[var(--bg-surface-2)] text-[var(--text-primary)] hover:border-[var(--accent)] hover:text-[var(--accent)]";
  };

  const onProviderSwitch = (provider: ModelProvider) => {
    onModelSettingsChange({
      ...modelSettings,
      active_provider: provider
    });
  };

  return (
    <motion.aside
      initial={{ x: 140, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: 140, opacity: 0 }}
      transition={{ type: "spring", damping: 26, stiffness: 300 }}
      className="pointer-events-auto h-full w-[78%] overflow-hidden border-l border-[var(--border-subtle)] bg-[var(--bg-surface-1)] shadow-[-24px_0_44px_-26px_rgba(0,0,0,0.48),24px_0_44px_-26px_rgba(0,0,0,0.24)] backdrop-blur-xl"
      data-open={open}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="flex h-11 items-center justify-between border-b border-[var(--border-subtle)] px-4">
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-1.5 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
          aria-label={t("back")}
          title={t("back")}
        >
          <ArrowRight className="h-4 w-4" />
          <span className="text-xs tracking-[0.1em] uppercase">{t("back")}</span>
        </button>
        <span className="text-xs tracking-[0.16em] text-[var(--text-secondary)] uppercase">
          {t("settingsTitle")}
        </span>
      </div>

      <div className="flex h-[calc(100%-44px)]">
        <aside className="w-[28%] border-r border-[var(--border-subtle)] bg-[var(--bg-surface-2)] p-3">
          <div className="mb-3 px-2 pt-1 text-xs tracking-[0.16em] text-[var(--text-secondary)] uppercase">
            {t("settings")}
          </div>
          <div className="relative mb-3">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--text-muted)]" />
            <CyberInput
              value={search}
              onChange={setSearch}
              placeholder={t("settingsSearchPlaceholder")}
              className="pl-8 text-xs"
            />
          </div>
          {filteredTabs.length === 0 ? (
            <div className="rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
              {t("noSettingsMatch")}
            </div>
          ) : (
            <div className="space-y-1">
              {filteredTabs.map((tab) => {
                const Icon = tab.icon;
                const active = activeTab === tab.key;
                return (
                  <button
                    key={tab.key}
                    type="button"
                    onClick={() => setActiveTab(tab.key)}
                    className={`relative flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                      active ? "text-[var(--accent)]" : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                    }`}
                  >
                    {active ? <span className="absolute left-0 h-4 w-[2px] rounded bg-[var(--accent)]" /> : null}
                    <Icon className="h-4 w-4" />
                    <span>{tab.label}</span>
                  </button>
                );
              })}
            </div>
          )}
        </aside>

        <section className="relative w-[72%] overflow-y-auto p-5">
          {activeTab === "basic" ? (
            <div className="pt-2">
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("basic")}
              </h3>
              <div className="space-y-4 pb-2">
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
                <div className="rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-sm text-[var(--text-primary)]">{t("watchRoot")}</div>
                      <div className="mt-1 truncate font-mono text-xs text-[var(--text-secondary)]" title={watchRoot}>
                        {watchRoot || "-"}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={onPickWatchRoot}
                      disabled={isPickingWatchRoot}
                      className="inline-flex items-center rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {isPickingWatchRoot ? t("watchRootPicking") : t("watchRootPick")}
                    </button>
                  </div>
                  <div className="mt-2 text-xs text-[var(--text-secondary)]">{t("watchRootRestartHint")}</div>
                </div>
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
              </div>
            </div>
          ) : null}

          {activeTab === "models" ? (
            <div className="pt-2">
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("models")}
              </h3>
              <div className="space-y-3">
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
                  <div className="rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-3">
                    <div className="flex items-center justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-sm text-[var(--text-primary)]">{t("modelLocalRoot")}</div>
                        <div className="mt-1 truncate font-mono text-xs text-[var(--text-secondary)]">
                          {modelSettings.local_profile.models_root || "-"}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => void onModelAction("refresh", onPickLocalModelsRoot)}
                          disabled={modelBusy}
                          className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                        >
                          {t("modelLocalRootPick")}
                        </button>
                        <button
                          type="button"
                          onClick={onClearLocalModelsRoot}
                          disabled={modelBusy}
                          className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                        >
                          {t("modelLocalRootClear")}
                        </button>
                      </div>
                    </div>
                  </div>
                ) : null}

                <div className="rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-3">
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
                </div>

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

                <div className="flex flex-wrap gap-2">
                  <motion.button
                    key={`probe-${actionState.probe.tick}`}
                    type="button"
                    onClick={() => void onModelAction("probe", onProbeModelProvider)}
                    disabled={modelBusy}
                    className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs transition disabled:opacity-60 ${buttonClassByState("probe")}`}
                    animate={
                      actionState.probe.phase === "success"
                        ? { scale: [1, 1.04, 1] }
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
                    key={`refresh-${actionState.refresh.tick}`}
                    type="button"
                    onClick={() => void onModelAction("refresh", onRefreshProviderModels)}
                    disabled={modelBusy}
                    className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs transition disabled:opacity-60 ${buttonClassByState("refresh")}`}
                    animate={
                      actionState.refresh.phase === "success"
                        ? { scale: [1, 1.04, 1] }
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
                    key={`save-${actionState.save.tick}`}
                    type="button"
                    onClick={() => void onModelAction("save", onSaveModelSettings)}
                    disabled={modelBusy}
                    className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs transition disabled:opacity-60 ${buttonClassByState("save", "primary")}`}
                    animate={
                      actionState.save.phase === "success"
                        ? { scale: [1, 1.04, 1] }
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
                      key={`pull-${actionState.pull.tick}`}
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
                      className={`inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs transition disabled:opacity-60 ${buttonClassByState("pull")}`}
                      animate={
                        actionState.pull.phase === "success"
                          ? { scale: [1, 1.04, 1] }
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
                  <div className="space-y-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                    <div className="text-[var(--text-primary)]">{t("modelStatusTitle")}</div>
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
                  </div>
                ) : null}
              </div>
            </div>
          ) : null}

          {activeTab === "advanced" ? (
            <div className="pt-2">
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("advanced")}
              </h3>
              <div className="rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] p-3 text-xs leading-5 text-[var(--text-secondary)]">
                {uiLang === "zh-CN"
                  ? "高级运行参数预留区，后续可扩展性能与调试选项。"
                  : "Reserved for advanced runtime and diagnostics options."}
              </div>
            </div>
          ) : null}

          {activeTab === "personalization" ? (
            <div className="pt-2">
              <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("personalization")}
              </h3>
              <div className="space-y-4">
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
              </div>
            </div>
          ) : null}
        </section>
      </div>
    </motion.aside>
  );
}
