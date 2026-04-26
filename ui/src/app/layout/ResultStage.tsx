import { AnimatePresence, motion } from "framer-motion";
import { LoaderCircle } from "lucide-react";
import { AnswerPanel } from "../panels/AnswerPanel";
import { CitationPanel } from "../panels/CitationPanel";
import { EvidencePanel } from "../panels/EvidencePanel";
import { MetricsPanel } from "../panels/MetricsPanel";
import { TrustPanel } from "../panels/TrustPanel";
import type { AskResponseStructured, MetricRow, VisibleCitation, VisibleEvidenceGroup } from "../types";
import { useI18n } from "../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type ResultStageProps = {
  isSearching: boolean;
  isSearchBarCollapsed: boolean;
  isSearchBarCompact: boolean;
  loading: boolean;
  error: string | null;
  answerResponse: AskResponseStructured | null;
  searchElapsedMs: number;
  lastSearchDurationMs: number | null;
  formatElapsed: (ms: number) => string;
  onResultScroll: (event: React.UIEvent<HTMLElement>) => void;
  onResultWheel: (event: React.WheelEvent<HTMLElement>) => void;
  visibleCitations: VisibleCitation[];
  expandedCitationKeys: Set<string>;
  onToggleCitationExpanded: (key: string) => void;
  visibleEvidenceGroups: VisibleEvidenceGroup[];
  expandedSourceKeys: Set<string>;
  onToggleSourceExpanded: (key: string) => void;
  onOpenSourceLocation: (path: string) => void;
  markdownRemarkPlugins: unknown[];
  markdownRehypePlugins: unknown[];
  metricRows: MetricRow[];
  measuredMetricsTotalMs: number;
  t: TranslateFn;
};

export function ResultStage({
  isSearching,
  isSearchBarCollapsed,
  isSearchBarCompact,
  loading,
  error,
  answerResponse,
  searchElapsedMs,
  lastSearchDurationMs,
  formatElapsed,
  onResultScroll,
  onResultWheel,
  visibleCitations,
  expandedCitationKeys,
  onToggleCitationExpanded,
  visibleEvidenceGroups,
  expandedSourceKeys,
  onToggleSourceExpanded,
  onOpenSourceLocation,
  markdownRemarkPlugins,
  markdownRehypePlugins,
  metricRows,
  measuredMetricsTotalMs,
  t
}: ResultStageProps) {
  return (
    <AnimatePresence>
      {isSearching && (
        <motion.section
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 12 }}
          transition={{ duration: 0.24, ease: "easeOut" }}
          style={{
            paddingTop: isSearchBarCollapsed ? 8 : isSearchBarCompact ? 12 : 16,
            transition: "padding-top 0.24s ease-out"
          }}
          className="no-scrollbar mx-auto h-full w-full max-w-3xl overflow-y-auto"
          onScroll={onResultScroll}
          onWheel={onResultWheel}
        >
          {loading && (
            <div className="flex items-center justify-between px-1 py-3 text-sm text-[var(--text-secondary)]">
              <div className="flex items-center gap-3">
                <LoaderCircle className="h-4 w-4 animate-spin text-[var(--accent)]" />
                {t("loading")}
              </div>
              <span className="text-xs text-[var(--text-muted)]">{formatElapsed(searchElapsedMs)}</span>
            </div>
          )}

          {!loading && error && (
            <div className="rounded-xl border border-red-500/40 bg-red-500/10 px-5 py-4 text-sm text-red-300">
              {error}
            </div>
          )}

          {!loading && !error && answerResponse && (
            <article className="pb-8">
              <AnswerPanel
                answerResponse={answerResponse}
                lastSearchDurationMs={lastSearchDurationMs}
                markdownRemarkPlugins={markdownRemarkPlugins}
                markdownRehypePlugins={markdownRehypePlugins}
                t={t}
              />
              <TrustPanel answerResponse={answerResponse} t={t} />
              <CitationPanel
                visibleCitations={visibleCitations}
                expandedCitationKeys={expandedCitationKeys}
                onToggleCitationExpanded={onToggleCitationExpanded}
                onOpenSourceLocation={(path) => void onOpenSourceLocation(path)}
                markdownRemarkPlugins={markdownRemarkPlugins}
                markdownRehypePlugins={markdownRehypePlugins}
                t={t}
              />
              <EvidencePanel
                visibleEvidenceGroups={visibleEvidenceGroups}
                expandedSourceKeys={expandedSourceKeys}
                onToggleSourceExpanded={onToggleSourceExpanded}
                onOpenSourceLocation={(path) => void onOpenSourceLocation(path)}
                markdownRemarkPlugins={markdownRemarkPlugins}
                markdownRehypePlugins={markdownRehypePlugins}
                t={t}
              />
              <MetricsPanel
                answerResponse={answerResponse}
                metricRows={metricRows}
                measuredMetricsTotalMs={measuredMetricsTotalMs}
                lastSearchDurationMs={lastSearchDurationMs}
                t={t}
              />
            </article>
          )}
        </motion.section>
      )}
    </AnimatePresence>
  );
}
