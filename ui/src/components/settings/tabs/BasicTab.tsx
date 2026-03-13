import { motion } from "framer-motion";
import { AnimatedPanel, AnimatedPressButton, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { CyberToggle } from "../../UI";
import { LanguageSwitch, SelectionChips, SettingCard } from "../controls";
import { useI18n, type Language } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type BasicTabProps = {
  t: TranslateFn;
  uiLang: Language;
  aiLang: Language;
  onUiLangChange: (lang: Language) => void;
  onAiLangChange: (lang: Language) => void;
  retrieveTopK: number;
  onRetrieveTopKChange: (value: number) => void;
  watchRoot: string;
  isPickingWatchRoot: boolean;
  onPickWatchRoot: () => void;
  autoSyncDaemon: boolean;
  onAutoSyncDaemonChange: (value: boolean) => void;
  graphRagInfer: boolean;
  onGraphRagInferChange: (value: boolean) => void;
};

export function BasicTab({
  t,
  uiLang,
  aiLang,
  onUiLangChange,
  onAiLangChange,
  retrieveTopK,
  onRetrieveTopKChange,
  watchRoot,
  isPickingWatchRoot,
  onPickWatchRoot,
  autoSyncDaemon,
  onAutoSyncDaemonChange,
  graphRagInfer,
  onGraphRagInferChange
}: BasicTabProps) {
  return (
    <motion.div
      key="settings-tab-basic"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("basic")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4 pb-2">
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
            onChange={onAutoSyncDaemonChange}
            ariaLabel={t("autoSyncDaemon")}
          />
        </SettingCard>
        <SettingCard title={t("graphRagInfer")} description={t("graphRagInferDesc")}>
          <CyberToggle checked={graphRagInfer} onChange={onGraphRagInferChange} ariaLabel={t("graphRagInfer")} />
        </SettingCard>
      </motion.div>
    </motion.div>
  );
}
