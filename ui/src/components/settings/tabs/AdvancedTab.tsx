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
  uiLang: string;
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

function isModelConnectionProblem(message: string): boolean {
  const normalized = message.toLowerCase();
  return [
    "embedding request failed",
    "connection refused",
    "actively refused",
    "failed to connect",
    "error sending request",
    "timed out",
    "timeout",
    "tcp connect error",
    "connectex"
  ].some((pattern) => normalized.includes(pattern));
}

function formatIndexingHealthMessage(
  message: string | null | undefined,
  uiLang: string,
  fallback: string
): string {
  const raw = message?.trim();
  if (!raw) return fallback;
  const lower = raw.toLowerCase();
  if (isModelConnectionProblem(raw)) {
    return uiLang === "zh-CN"
      ? "向量模型未启动或端口不可连接。请先启动 embedding 模型，然后点击继续索引或重建索引。"
      : "Embedding model is not running or the port is unreachable. Start it, then resume or rebuild indexing.";
  }
  if (lower.includes("retryable_files_remaining")) {
    return uiLang === "zh-CN"
      ? "还有文件等待重试。通常是模型未启动、端口不通或文件暂时不可读。"
      : "Some files are waiting for retry. This is usually caused by a stopped model, blocked port, or temporarily unreadable files.";
  }
  if (lower.includes("index is not ready")) {
    return uiLang === "zh-CN"
      ? "索引还没完成，当前不能检索。请先启动模型并继续索引。"
      : "Indexing is not finished yet. Start the model and continue indexing before searching.";
  }
  if (raw.includes("索引不可用")) {
    return uiLang === "zh-CN"
      ? "索引还没完成。如果刚才模型没启动，请先启动模型，然后点击继续索引或重建索引。"
      : "Indexing is not finished yet. If the model was stopped, start it and then resume or rebuild indexing.";
  }
  return raw;
}

export function AdvancedTab({
  t,
  uiLang,
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
  const readableLastError = formatIndexingHealthMessage(
    indexingStatus?.last_error,
    uiLang,
    t("indexingNoError")
  );
  const readableRebuildReason = formatIndexingHealthMessage(
    indexingStatus?.rebuild_reason,
    uiLang,
    t("indexingNoError")
  );

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
          {/* Progress bar */}
          {indexingStatus && indexingStatus.phase !== "idle" ? (
            <div className="mb-3">
              <div className="flex items-center justify-between text-[11px] mb-1">
                <span className={`font-medium ${
                  indexingStatus.progress_percent < 33
                    ? "text-red-400"
                    : indexingStatus.progress_percent < 66
                      ? "text-amber-400"
                      : indexingStatus.progress_percent < 100
                        ? "text-emerald-400"
                        : "text-sky-400"
                }`}>
                  {indexingStatus.progress_percent < 33
                    ? "扫描文档中…（暂不可用）"
                    : indexingStatus.progress_percent < 66
                      ? "建立向量索引中…（即将可用）"
                      : indexingStatus.progress_percent < 100
                        ? "构建知识图谱中…（已可使用）"
                        : "索引完全就绪"}
                </span>
                <span className="font-mono text-[var(--text-muted)]">{indexingStatus.progress_percent}%</span>
              </div>
              {/* Segmented progress bar with milestone markers */}
              <div className="relative h-2 w-full">
                {/* Background track with segments */}
                <div className="absolute inset-0 flex rounded-full overflow-hidden">
                  <div className="w-[33%] h-full bg-red-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[33%] h-full bg-amber-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[34%] h-full bg-emerald-500/15" />
                </div>
                {/* Fill bar */}
                <motion.div
                  className={`absolute top-0 left-0 h-full rounded-full ${
                    indexingStatus.progress_percent < 33
                      ? "bg-red-400"
                      : indexingStatus.progress_percent < 66
                        ? "bg-amber-400"
                        : indexingStatus.progress_percent < 100
                          ? "bg-emerald-400"
                          : "bg-sky-400"
                  }`}
                  initial={{ width: 0 }}
                  animate={{ width: `${indexingStatus.progress_percent}%` }}
                  transition={{ duration: 0.5, ease: "easeOut" }}
                />
                {/* Milestone markers */}
                <div className="absolute top-0 left-[33%] h-full w-px bg-[var(--bg-surface-1)]" />
                <div className="absolute top-0 left-[66%] h-full w-px bg-[var(--bg-surface-1)]" />
              </div>
              {/* Milestone labels */}
              <div className="mt-1 flex justify-between text-[9px] text-[var(--text-muted)]">
                <span className={indexingStatus.progress_percent >= 33 ? "text-amber-400 font-medium" : ""}>可用</span>
                <span className={indexingStatus.progress_percent >= 66 ? "text-emerald-400 font-medium" : ""}>搜索</span>
                <span className={indexingStatus.progress_percent >= 100 ? "text-sky-400 font-medium" : ""}>优化</span>
              </div>
              <div className="mt-1 flex gap-3 text-[10px] text-[var(--text-muted)]">
                <span>文档 {indexingStatus.indexed_docs}/{indexingStatus.total_docs}</span>
                <span>分块 {indexingStatus.indexed_chunks}/{indexingStatus.total_chunks}</span>
                <span>图谱 {indexingStatus.graphed_chunks}</span>
              </div>
            </div>
          ) : null}
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
            <div className="text-[var(--text-primary)]">{readableLastError}</div>
            <div>{t("indexingRebuildState")}</div>
            <div className="text-[var(--text-primary)]">{indexingRebuildLabel}</div>
            <div>{t("indexingRebuildReason")}</div>
            <div className="text-[var(--text-primary)]">{readableRebuildReason}</div>
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
