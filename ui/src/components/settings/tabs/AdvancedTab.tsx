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
  indexingModelBlocked: boolean;
  stableActionButtonClass: string;
  indexingButtonClass: (key: IndexingActionKey) => string;
  indexingAction: { key: IndexingActionKey | null; phase: ActionPhase; tick: number };
  onIndexingAction: (key: IndexingActionKey, action: () => Promise<void>) => Promise<void>;
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
  indexingModelBlocked,
  stableActionButtonClass,
  indexingButtonClass,
  indexingAction,
  onIndexingAction,
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
  const shownProgress = indexingModelBlocked ? 0 : (indexingStatus?.progress_percent ?? 0);
  const rebuildState = (indexingStatus?.rebuild_state ?? "ready").toLowerCase();
  const retryableFilesRemaining = Boolean(indexingStatus?.rebuild_reason?.includes("retryable_files_remaining"));
  const hasIndexedChunks = (indexingStatus?.indexed_chunks ?? 0) > 0;
  const searchReady = (rebuildState === "ready" || (retryableFilesRemaining && hasIndexedChunks)) && !indexingModelBlocked;
  const phase = (indexingStatus?.phase ?? "idle").toLowerCase();
  const optimizing = searchReady && (phase === "graphing" || (indexingStatus?.graph_backlog ?? 0) > 0);
  const progressTone = shownProgress < 66 ? "building" : shownProgress < 100 ? "searchable" : "optimized";
  const progressText = indexingModelBlocked
    ? uiLang === "zh-CN"
      ? "等待向量模型启动"
      : "Waiting for embedding model"
    : optimizing
      ? uiLang === "zh-CN"
        ? "可搜索，正在继续优化图谱..."
        : "Search is available. Optimizing graph..."
      : searchReady
        ? uiLang === "zh-CN"
          ? "索引就绪，可以搜索"
          : "Index ready. Search is available"
      : phase === "graphing"
        ? uiLang === "zh-CN"
          ? "向量已完成，等待开放搜索..."
          : "Vectors are ready. Waiting to enable search..."
        : phase === "embedding"
          ? uiLang === "zh-CN"
            ? "建立向量索引中...（搜索暂不可用）"
            : "Building vector index... (search unavailable)"
          : phase === "scanning"
            ? uiLang === "zh-CN"
              ? "扫描文档中...（搜索暂不可用）"
              : "Scanning documents... (search unavailable)"
            : uiLang === "zh-CN"
              ? "索引重建中...（搜索暂不可用）"
              : "Index rebuild in progress... (search unavailable)";

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
          {indexingStatus && (indexingStatus.phase !== "idle" || indexingModelBlocked || !searchReady) ? (
            <div className="mb-3">
              <div className="flex items-center justify-between text-[11px] mb-1">
                <span className={`font-medium ${
                  progressTone === "building"
                    ? "text-amber-400"
                    : progressTone === "searchable"
                      ? "text-emerald-400"
                      : "text-sky-400"
                }`}>
                  {progressText}
                </span>
                <span className="font-mono text-[var(--text-muted)]">{shownProgress}%</span>
              </div>
              {/* Segmented progress bar with milestone markers */}
              <div className="relative h-2 w-full">
                {/* Background track with segments */}
                <div className="absolute inset-0 flex rounded-full overflow-hidden">
                  <div className="w-[66%] h-full bg-amber-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[17%] h-full bg-emerald-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[17%] h-full bg-sky-500/15" />
                </div>
                {/* Fill bar */}
                <motion.div
                  className={`absolute top-0 left-0 h-full rounded-full ${
                    progressTone === "building"
                        ? "bg-amber-400"
                      : progressTone === "searchable"
                          ? "bg-emerald-400"
                        : "bg-sky-400"
                  }`}
                  initial={{ width: 0 }}
                  animate={{ width: `${shownProgress}%` }}
                  transition={{ duration: 0.5, ease: "easeOut" }}
                />
                {/* Milestone markers */}
                <div className="absolute top-0 left-[66%] h-full w-px bg-[var(--bg-surface-1)]" />
                <div className="absolute top-0 left-[83%] h-full w-px bg-[var(--bg-surface-1)]" />
              </div>
              {/* Milestone labels */}
              <div className="mt-1 flex justify-between text-[9px] text-[var(--text-muted)]">
                <span className={shownProgress < 66 ? "text-amber-400 font-medium" : ""}>
                  {uiLang === "zh-CN" ? "构建" : "Build"}
                </span>
                <span className={searchReady ? "text-sky-400 font-medium" : ""}>
                  {uiLang === "zh-CN" ? "可搜索" : "Search ready"}
                </span>
                <span className={shownProgress >= 100 ? "text-sky-400 font-medium" : ""}>
                  {uiLang === "zh-CN" ? "优化" : "Optimize"}
                </span>
              </div>
              {!searchReady ? (
                <div className="mt-2 rounded-md border border-amber-400/25 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-500">
                  {uiLang === "zh-CN"
                    ? "黄色阶段表示向量索引仍在构建，暂时不能搜索。进入绿色后即可搜索，蓝色阶段会继续做图谱优化。"
                    : "Yellow means the vector index is still building. Search starts in green; blue continues graph optimization."}
                </div>
              ) : retryableFilesRemaining ? (
                <div className="mt-2 rounded-md border border-amber-400/25 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-500">
                  {uiLang === "zh-CN"
                    ? "已有分块可以搜索；少量失败文件会继续重试，不再阻塞检索。"
                    : "Search is available from indexed chunks; remaining failed files will be retried without blocking retrieval."}
                </div>
              ) : null}
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
