import { motion } from "framer-motion";
import { LoaderCircle } from "lucide-react";
import { AnimatedPanel, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { SelectionChips, SettingCard } from "../controls";
import type {
  IndexingMode,
  IndexingStatusDto,
  LocalModelRuntimeStatusesDto,
  ResourceBudget
} from "../types";
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
  localModelRuntimeStatuses: LocalModelRuntimeStatusesDto | null;
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
  localModelRuntimeStatuses,
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
  const embedRuntime = localModelRuntimeStatuses?.roles.find((role) => role.role === "embed");
  const rebuildState = (indexingStatus?.rebuild_state ?? "ready").toLowerCase();
  const retryableFilesRemaining = Boolean(indexingStatus?.rebuild_reason?.includes("retryable_files_remaining"));
  const hasIndexedDocs = (indexingStatus?.indexed_docs ?? 0) > 0;
  const hasIndexedChunks = (indexingStatus?.indexed_chunks ?? 0) > 0;
  const hasIndexedGraph = (indexingStatus?.graphed_chunks ?? 0) > 0;
  const hasSearchableIndex = hasIndexedDocs || hasIndexedChunks || hasIndexedGraph;
  const searchReady =
    ((rebuildState === "ready" && hasSearchableIndex) || (retryableFilesRemaining && hasIndexedChunks)) &&
    !indexingModelBlocked;
  const phase = (indexingStatus?.phase ?? "idle").toLowerCase();
  const rawProgress = indexingStatus?.progress_percent ?? 0;
  const graphedChunks = indexingStatus?.graphed_chunks ?? 0;
  const graphBacklog = indexingStatus?.graph_backlog ?? 0;
  const graphTotal = graphedChunks + graphBacklog;
  const optimizing = searchReady && (phase === "graphing" || (indexingStatus?.graph_backlog ?? 0) > 0);
  const optimizeProgressPercent =
    graphTotal > 0 ? Math.min(100, 83 + (graphedChunks / graphTotal) * 17) : (searchReady ? 100 : 0);
  const shownProgress = indexingModelBlocked
    ? 0
    : optimizing
      ? optimizeProgressPercent
      : Math.max(searchReady ? 83 : 0, rawProgress);
  const optimizationComplete = searchReady && !optimizing && shownProgress >= 100;
  const buildFill = searchReady ? 66 : Math.min(shownProgress, 66);
  const searchFill = searchReady ? 17 : Math.min(Math.max(shownProgress - 66, 0), 17);
  const optimizeFill = searchReady ? Math.min(Math.max(shownProgress - 83, 0), 17) : 0;
  const shownProgressLabel = `${shownProgress.toFixed(2)}%`;
  const progressTone = indexingModelBlocked
    ? "building"
    : !searchReady
      ? "building"
      : optimizationComplete || optimizing
        ? "optimized"
        : "searchable";
  const progressText = indexingModelBlocked
    ? uiLang === "zh-CN"
      ? embedRuntime?.state?.toLowerCase() === "starting"
        ? "向量模型启动中，等待恢复索引"
        : "向量模型离线，等待恢复"
      : embedRuntime?.state?.toLowerCase() === "starting"
        ? "Embedding model is starting"
        : "Embedding model offline"
    : optimizing
      ? uiLang === "zh-CN"
        ? "可搜索，后台正在持续优化图谱"
        : "Search is available. Optimizing graph in background."
      : optimizationComplete
        ? uiLang === "zh-CN"
          ? "优化完毕，性能最佳"
          : "Optimization complete. Best performance."
      : searchReady
        ? uiLang === "zh-CN"
          ? "可搜索，状态监测中"
          : "Search ready. Monitoring status."
      : indexingStatus?.paused
        ? uiLang === "zh-CN"
          ? "后台构建已暂停"
          : "Background indexing is paused"
      : phase === "graphing"
        ? uiLang === "zh-CN"
          ? "已进入图谱优化阶段"
          : "Graph optimization in progress"
        : phase === "embedding"
          ? uiLang === "zh-CN"
            ? "正在构建向量索引，暂不可搜索"
            : "Building vector index. Search is unavailable."
          : phase === "scanning"
            ? uiLang === "zh-CN"
              ? "正在扫描文档，暂不可搜索"
              : "Scanning documents. Search is unavailable."
            : uiLang === "zh-CN"
              ? "等待继续构建索引"
              : "Waiting to continue indexing";

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
          {indexingStatus ? (
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
                <span className="font-mono text-[var(--text-muted)]">{shownProgressLabel}</span>
              </div>
              {/* Segmented progress bar with milestone markers */}
              <div className="relative h-2 w-full">
                {/* Background track with segments */}
                <div className="absolute inset-0 flex rounded-full overflow-hidden">
                  <div className="w-[66%] h-full bg-amber-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[17%] h-full bg-emerald-500/15 border-r border-[var(--bg-surface-1)]" />
                  <div className="w-[17%] h-full bg-sky-500/15" />
                </div>
                {/* Build fill */}
                <motion.div
                  className="absolute top-0 left-0 h-full rounded-l-full bg-amber-400"
                  initial={{ width: 0 }}
                  animate={{ width: `${buildFill}%` }}
                  transition={{ duration: 0.35, ease: "easeOut" }}
                />
                {/* Search-ready fill */}
                <motion.div
                  className="absolute top-0 left-[66%] h-full bg-emerald-400"
                  initial={{ width: 0 }}
                  animate={{ width: `${searchFill}%` }}
                  transition={{ duration: 0.35, ease: "easeOut" }}
                />
                {/* Optimize fill */}
                <motion.div
                  className="absolute top-0 left-[83%] h-full rounded-r-full bg-sky-400"
                  initial={{ width: 0 }}
                  animate={{ width: `${optimizeFill}%` }}
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
                <span className={searchReady ? "text-emerald-400 font-medium" : ""}>
                  {uiLang === "zh-CN" ? "可搜索" : "Search ready"}
                </span>
                <span className={optimizing || optimizationComplete ? "text-sky-400 font-medium" : ""}>
                  {uiLang === "zh-CN" ? "优化" : "Optimize"}
                </span>
              </div>
              {!searchReady ? (
                <div className="mt-2 rounded-md border border-amber-400/25 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-500">
                  {uiLang === "zh-CN"
                    ? "黄色表示主索引仍在构建，暂不可搜索。进入绿色后即可搜索，蓝色阶段会继续做图谱优化。"
                    : "Yellow means the main index is still building. Search starts in green, and blue continues graph optimization."}
                </div>
              ) : retryableFilesRemaining ? (
                <div className="mt-2 rounded-md border border-amber-400/25 bg-amber-500/10 px-2 py-1.5 text-[11px] text-amber-500">
                  {uiLang === "zh-CN"
                    ? "当前已可搜索；剩余失败文件会继续重试，不再阻塞主检索。"
                    : "Search is already available. Remaining failed files will retry without blocking retrieval."}
                </div>
              ) : (
                <div className="mt-2 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1.5 text-[11px] text-[var(--text-secondary)]">
                  {uiLang === "zh-CN"
                    ? "进度条会常驻显示当前构建、可搜索和图谱优化状态，并持续监测模型与索引变化。"
                    : "This bar stays visible and continuously monitors build, search-ready, and graph optimization state."}
                </div>
              )}
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
