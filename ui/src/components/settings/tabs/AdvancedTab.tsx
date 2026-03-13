import { motion } from "framer-motion";
import { LoaderCircle } from "lucide-react";
import { AnimatedPanel, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { SelectionChips, SettingCard } from "../controls";
import type { IndexingMode, IndexingStatusDto, ResourceBudget } from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];
export type IndexingActionKey = "saveIndexing" | "triggerReindex" | "pauseResume";
type ActionPhase = "idle" | "running" | "success" | "error";

type AdvancedTabProps = {
  t: TranslateFn;
  indexingMode: IndexingMode;
  onIndexingModeChange: (mode: IndexingMode) => void;
  resourceBudget: ResourceBudget;
  onResourceBudgetChange: (budget: ResourceBudget) => void;
  scheduleStart: string;
  scheduleEnd: string;
  onScheduleStartChange: (value: string) => void;
  onScheduleEndChange: (value: string) => void;
  indexingStatus: IndexingStatusDto | null;
  indexingBusy: boolean;
  indexingPhaseLabel: string;
  indexingRebuildLabel: string;
  lastScanLabel: string;
  etaLabel: string;
  stableActionButtonClass: string;
  indexingButtonClass: (key: IndexingActionKey) => string;
  indexingAction: { key: IndexingActionKey | null; phase: ActionPhase; tick: number };
  onIndexingAction: (key: IndexingActionKey, action: () => Promise<void>) => Promise<void>;
  onSaveIndexingConfig: () => Promise<void>;
  onTriggerReindex: () => Promise<void>;
  onPauseIndexing: () => Promise<void>;
  onResumeIndexing: () => Promise<void>;
};

export function AdvancedTab({
  t,
  indexingMode,
  onIndexingModeChange,
  resourceBudget,
  onResourceBudgetChange,
  scheduleStart,
  scheduleEnd,
  onScheduleStartChange,
  onScheduleEndChange,
  indexingStatus,
  indexingBusy,
  indexingPhaseLabel,
  indexingRebuildLabel,
  lastScanLabel,
  etaLabel,
  stableActionButtonClass,
  indexingButtonClass,
  indexingAction,
  onIndexingAction,
  onSaveIndexingConfig,
  onTriggerReindex,
  onPauseIndexing,
  onResumeIndexing
}: AdvancedTabProps) {
  return (
    <motion.div
      key="settings-tab-advanced"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("advanced")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4">
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
                <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("scheduleWindowStart")}</div>
                <input
                  type="time"
                  value={scheduleStart}
                  onChange={(event) => onScheduleStartChange(event.target.value)}
                  className="h-9 w-full rounded-md border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                />
              </div>
              <div>
                <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("scheduleWindowEnd")}</div>
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
            <div className="text-[var(--text-primary)]">{indexingStatus?.last_error?.trim() || t("indexingNoError")}</div>
            <div>{t("indexingRebuildState")}</div>
            <div className="text-[var(--text-primary)]">{indexingRebuildLabel}</div>
            <div>{t("indexingRebuildReason")}</div>
            <div className="text-[var(--text-primary)]">{indexingStatus?.rebuild_reason?.trim() || t("indexingNoError")}</div>
            <div>{t("indexingIndexVersion")}</div>
            <div className="text-[var(--text-primary)]">{indexingStatus?.index_format_version ?? 0}</div>
            <div>{t("indexingParserVersion")}</div>
            <div className="text-[var(--text-primary)]">{indexingStatus?.parser_format_version ?? 0}</div>
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
  );
}
