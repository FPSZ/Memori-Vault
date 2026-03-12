import { formatElapsed, formatMetricDuration, formatQueryFlag } from "../formatters";
import type { AskResponseStructured, MetricRow, Translate } from "../types";

type MetricsPanelProps = {
  answerResponse: AskResponseStructured;
  metricRows: MetricRow[];
  measuredMetricsTotalMs: number;
  lastSearchDurationMs: number | null;
  t: Translate;
};

export function MetricsPanel({
  answerResponse,
  metricRows,
  measuredMetricsTotalMs,
  lastSearchDurationMs,
  t
}: MetricsPanelProps) {
  return (
    <section className="mt-8">
      <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
        {t("retrievalMetricsTitle")}
      </div>
      {answerResponse.metrics.query_flags.length > 0 ? (
        <div className="mb-3 flex flex-wrap gap-2 text-[11px]">
          {answerResponse.metrics.query_flags.map((flag) => (
            <span
              key={flag}
              className="rounded-full border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-0.5 text-[var(--text-secondary)]"
            >
              {formatQueryFlag(flag, t)}
            </span>
          ))}
        </div>
      ) : null}
      <div className="mb-4 flex flex-wrap items-center gap-x-5 gap-y-1 text-xs text-[var(--text-secondary)]">
        <span>
          {t("metricTotal")} <span className="font-mono text-[var(--text-primary)]">{lastSearchDurationMs !== null ? formatElapsed(lastSearchDurationMs) : "-"}</span>
        </span>
        <span>
          {t("metricMeasured")} <span className="font-mono text-[var(--text-primary)]">{formatMetricDuration(measuredMetricsTotalMs)}</span>
        </span>
        {lastSearchDurationMs !== null && lastSearchDurationMs > measuredMetricsTotalMs ? (
          <span>
            {t("metricUntracked")} <span className="font-mono text-[var(--text-primary)]">{formatMetricDuration(lastSearchDurationMs - measuredMetricsTotalMs)}</span>
          </span>
        ) : null}
      </div>
      <div className="space-y-3">
        {metricRows.map((metric, index) => {
          const maxMetricValue = metricRows[0]?.value ?? 1;
          const widthPercent = Math.max(6, (metric.value / maxMetricValue) * 100);
          return (
            <div key={metric.key} className="flex items-center gap-3 text-xs text-[var(--text-secondary)]">
              <span className="w-6 shrink-0 text-right font-mono text-[10px] text-[var(--text-muted)]">{index + 1}</span>
              <span className="w-28 shrink-0 truncate text-[var(--text-secondary)] md:w-36">{metric.label}</span>
              <div className="min-w-0 flex-1">
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--accent)_10%,var(--bg-canvas)_90%)]">
                  <div className="h-full rounded-full bg-[var(--accent)]" style={{ width: `${widthPercent}%` }} />
                </div>
              </div>
              <span className="w-14 shrink-0 text-right font-mono text-[var(--text-primary)] md:w-16">
                {formatMetricDuration(metric.value)}
              </span>
            </div>
          );
        })}
      </div>
      <div className="mt-3 text-[11px] text-[var(--text-muted)]">{t("metricsNote")}</div>
    </section>
  );
}
