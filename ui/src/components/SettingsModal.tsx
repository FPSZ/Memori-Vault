import { motion } from "framer-motion";
import { ArrowRight, Cpu, Database, Palette, Search, Settings } from "lucide-react";
import { ReactNode, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "../i18n";
import { useI18n } from "../i18n";
import { CyberInput, CyberToggle } from "./UI";

export type FontPreset = "system" | "neo" | "mono";
export type FontScale = "s" | "m" | "l";

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
  fontPreset: FontPreset;
  onFontPresetChange: (preset: FontPreset) => void;
  fontScale: FontScale;
  onFontScaleChange: (scale: FontScale) => void;
  themeModeState: "a" | "b";
  onThemeModeChange: (mode: "a" | "b") => void;
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
    <div className="inline-flex items-center rounded-lg border border-[#30363d] bg-[#0d1117]/70 p-1">
      <button
        type="button"
        onClick={() => onChange("zh-CN")}
        className={`rounded-md px-2 py-1 text-xs font-mono transition ${
          value === "zh-CN" ? "bg-[var(--accent-soft)] text-[var(--accent)]" : "text-[#8b949e] hover:text-[#c9d1d9]"
        }`}
      >
        CN
      </button>
      <button
        type="button"
        onClick={() => onChange("en-US")}
        className={`rounded-md px-2 py-1 text-xs font-mono transition ${
          value === "en-US" ? "bg-[var(--accent-soft)] text-[var(--accent)]" : "text-[#8b949e] hover:text-[#c9d1d9]"
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
              : "border-[#30363d] bg-[#0d1117]/50 text-[#8b949e] hover:text-[#c9d1d9]"
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
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-[#30363d] bg-[#0d1117]/40 px-3 py-3">
      <div className="min-w-0">
        <div className="text-sm text-[#c9d1d9]">{title}</div>
        <div className="mt-1 text-xs text-[#8b949e]">{description}</div>
      </div>
      <div className="shrink-0">{children}</div>
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
  fontPreset,
  onFontPresetChange,
  fontScale,
  onFontScaleChange,
  themeModeState,
  onThemeModeChange
}: SettingsModalProps) {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const [search, setSearch] = useState("");
  const [aiMatchedKeys, setAiMatchedKeys] = useState<TabKey[] | null>(null);
  const [autoSyncDaemon, setAutoSyncDaemon] = useState(true);
  const [graphRagInfer, setGraphRagInfer] = useState(true);
  const [graphModel, setGraphModel] = useState("qwen2.5:7b");

  const tabMeta = useMemo(
    () => [
      {
        key: "basic" as const,
        label: t("basic"),
        icon: Cpu,
        keywords: [t("uiLanguage"), t("aiReplyLanguage"), t("watchRoot"), t("autoSyncDaemon"), t("graphRagInfer")]
      },
      {
        key: "models" as const,
        label: t("models"),
        icon: Settings,
        keywords: [t("graphExtractorModel")]
      },
      {
        key: "advanced" as const,
        label: t("advanced"),
        icon: Database,
        keywords: ["diagnostics", "runtime"]
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
    if (!q) {
      return tabMeta;
    }
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

      void invoke<string[]>("rank_settings_query", {
        query,
        candidates,
        lang: uiLang
      })
        .then((keys) => {
          if (cancelled) {
            return;
          }
          const valid = keys.filter((key): key is TabKey =>
            tabMeta.some((tab) => tab.key === key)
          );
          setAiMatchedKeys(valid.length > 0 ? valid : null);
        })
        .catch(() => {
          if (!cancelled) {
            // AI 不可用时自动回落本地关键词匹配。
            setAiMatchedKeys(null);
          }
        });
    }, 280);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [search, tabMeta, uiLang]);

  const filteredTabs = useMemo(() => {
    if (!search.trim()) {
      return tabMeta;
    }
    if (aiMatchedKeys && aiMatchedKeys.length > 0) {
      const map = new Map(tabMeta.map((tab) => [tab.key, tab]));
      return aiMatchedKeys
        .map((key) => map.get(key))
        .filter((tab): tab is (typeof tabMeta)[number] => Boolean(tab));
    }
    return localFilteredTabs;
  }, [aiMatchedKeys, localFilteredTabs, search, tabMeta]);

  useEffect(() => {
    if (filteredTabs.length === 0) {
      return;
    }
    if (!filteredTabs.some((tab) => tab.key === activeTab)) {
      setActiveTab(filteredTabs[0].key);
    }
  }, [activeTab, filteredTabs]);

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
  return (
    <motion.aside
      initial={{ x: 56, opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: 56, opacity: 0 }}
      transition={{ type: "spring", damping: 26, stiffness: 300 }}
      className="pointer-events-auto h-full w-[78%] overflow-hidden border-l border-white/10 bg-[#161b22]/93 shadow-[-24px_0_60px_-20px_rgba(0,0,0,0.7),_0_0_30px_rgba(88,166,255,0.08)] backdrop-blur-xl"
      data-open={open}
    >
      <div className="flex h-11 items-center justify-between border-b border-white/10 px-4">
        <button
          type="button"
          onClick={onBack}
          className="inline-flex items-center gap-1.5 text-[#8b949e] transition hover:text-[#c9d1d9]"
          aria-label={t("back")}
          title={t("back")}
        >
          <ArrowRight className="h-4 w-4" />
          <span className="text-xs font-mono tracking-[0.1em] uppercase">{t("back")}</span>
        </button>
        <span className="text-xs font-mono tracking-[0.16em] text-[#8b949e] uppercase">
          {t("settingsTitle")}
        </span>
      </div>

      <div className="flex h-[calc(100%-44px)]">
        <aside className="w-[28%] border-r border-white/10 bg-[#0d1117]/50 p-3">
          <div className="mb-3 px-2 pt-1 text-xs font-mono tracking-[0.16em] text-[#8b949e] uppercase">
            {t("settings")}
          </div>

          <div className="relative mb-3">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[#6e7681]" />
            <CyberInput
              value={search}
              onChange={setSearch}
              placeholder={t("settingsSearchPlaceholder")}
              className="pl-8 text-xs"
            />
          </div>

          {filteredTabs.length === 0 ? (
            <div className="rounded-lg border border-[#30363d] bg-[#0d1117]/40 px-3 py-2 text-xs text-[#8b949e]">
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
                      active ? "text-[var(--accent)]" : "text-[#8b949e] hover:text-[#c9d1d9]"
                    }`}
                  >
                    {active && <span className="absolute left-0 h-4 w-[2px] rounded bg-[var(--accent)]" />}
                    <Icon className="h-4 w-4" />
                    <span>{tab.label}</span>
                  </button>
                );
              })}
            </div>
          )}
        </aside>

        <section className="relative w-[72%] overflow-y-auto p-5">
          {activeTab === "basic" && (
            <div className="pt-2">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("basic")}
              </h3>

              <div className="space-y-4 pb-2">
                <SettingCard
                  title={t("uiLanguage")}
                  description={uiLang === "zh-CN" ? "界面文案实时切换。" : "Switch UI copy instantly."}
                >
                  <LanguageSwitch value={uiLang} onChange={onUiLangChange} />
                </SettingCard>

                <SettingCard
                  title={t("aiReplyLanguage")}
                  description={
                    aiLang === "zh-CN"
                      ? "后续回答优先使用中文。"
                      : "Next responses will prefer English."
                  }
                >
                  <LanguageSwitch value={aiLang} onChange={onAiLangChange} />
                </SettingCard>

                <div className="rounded-lg border border-[#30363d] bg-[#0d1117]/40 px-3 py-3">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <div className="text-sm text-[#c9d1d9]">{t("watchRoot")}</div>
                      <div className="mt-1 truncate font-mono text-xs text-[#8b949e]" title={watchRoot}>
                        {watchRoot || "-"}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={onPickWatchRoot}
                      disabled={isPickingWatchRoot}
                      className="inline-flex items-center rounded-md border border-[#30363d] bg-[#0d1117]/60 px-3 py-1.5 text-xs text-[#c9d1d9] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:cursor-not-allowed disabled:opacity-60"
                    >
                      {isPickingWatchRoot ? t("watchRootPicking") : t("watchRootPick")}
                    </button>
                  </div>
                  <div className="mt-2 text-xs text-[#8b949e]">{t("watchRootRestartHint")}</div>
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
          )}

          {activeTab === "models" && (
            <div className="pt-2">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("models")}
              </h3>

              <label className="mb-2 block text-xs font-mono tracking-wide text-[#8b949e]">
                {t("graphExtractorModel")}
              </label>
              <CyberInput value={graphModel} onChange={setGraphModel} placeholder="qwen2.5:7b" />
              <p className="mt-3 text-xs leading-5 text-[#8b949e]">{t("graphExtractorModelDesc")}</p>
            </div>
          )}

          {activeTab === "advanced" && (
            <div className="pt-2">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("advanced")}
              </h3>

              <div className="rounded-lg border border-[#30363d] bg-[#0d1117]/40 p-3 text-xs leading-5 text-[#8b949e]">
                {uiLang === "zh-CN"
                  ? "保留高级运行参数入口，后续用于诊断与性能开关。"
                  : "Advanced runtime controls will be placed here later."}
              </div>
            </div>
          )}

          {activeTab === "personalization" && (
            <div className="pt-2">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[var(--accent)] uppercase">
                {t("personalization")}
              </h3>

              <div className="space-y-4">
                <SettingCard title={t("fontPreset")} description={t("fontPresetSystem")}>
                  <SelectionChips
                    value={fontPreset}
                    onChange={onFontPresetChange}
                    options={fontPresetOptions}
                  />
                </SettingCard>

                <SettingCard title={t("fontSize")} description={t("fontSizeM")}>
                  <SelectionChips
                    value={fontScale}
                    onChange={onFontScaleChange}
                    options={fontScaleOptions}
                  />
                </SettingCard>

                <SettingCard title={t("themeToggle")} description={t("themeToggleDesc")}>
                  <SelectionChips
                    value={themeModeState}
                    onChange={onThemeModeChange}
                    options={[
                      { value: "a", label: t("themeModeA") },
                      { value: "b", label: t("themeModeB") }
                    ]}
                  />
                </SettingCard>
              </div>
            </div>
          )}
        </section>
      </div>
    </motion.aside>
  );
}
