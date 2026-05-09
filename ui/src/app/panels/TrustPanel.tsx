import { useMemo, useState } from "react";

import type { AskResponseStructured, Translate } from "../types";

type TrustPanelProps = {
  answerResponse: AskResponseStructured;
  t: Translate;
};

export function TrustPanel({ answerResponse, t }: TrustPanelProps) {
  const [expanded, setExpanded] = useState(false);
  const budget = answerResponse.context_budget_report;
  const sourceGroups = answerResponse.source_groups ?? [];
  const memories = answerResponse.memory_context ?? [];
  const gating = answerResponse.metrics;

  const summaryItems = useMemo(() => {
    const items: string[] = [];
    if (answerResponse.failure_class) {
      items.push(`failure: ${formatFailureClass(answerResponse.failure_class)}`);
    }
    if (gating.gating_score !== undefined) {
      items.push(`gate: ${gating.gating_score}/${gating.gating_threshold ?? "-"}`);
    }
    if (sourceGroups.length > 0) {
      items.push(`sources: ${sourceGroups.length}`);
    }
    if (memories.length > 0) {
      items.push(`memory: ${memories.length}`);
    }
    if (budget) {
      items.push(`budget: ${budget.used_by_documents}/${budget.token_budget}`);
    }
    return items;
  }, [
    answerResponse.failure_class,
    budget,
    gating.gating_score,
    gating.gating_threshold,
    memories.length,
    sourceGroups.length,
  ]);

  if (
    !budget &&
    sourceGroups.length === 0 &&
    memories.length === 0 &&
    !answerResponse.failure_class &&
    gating.gating_score === undefined
  ) {
    return null;
  }

  return (
    <section className="mt-6 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
      <button
        type="button"
        onClick={() => setExpanded((value) => !value)}
        className="flex w-full items-center justify-between gap-3 text-left"
      >
        <div className="min-w-0">
          <div className="text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">
            {t("trustPanelTitle")}
          </div>
          {summaryItems.length > 0 ? (
            <div className="mt-1 truncate text-xs text-[var(--text-muted)]">{summaryItems.join(" · ")}</div>
          ) : null}
        </div>
        <span className="shrink-0 text-xs text-[var(--text-secondary)]">
          {expanded ? t("trustPanelCollapse") : t("trustPanelExpand")}
        </span>
      </button>

      {!expanded ? null : (
        <>
          <div className="mt-3 grid gap-3 text-xs text-[var(--text-secondary)] md:grid-cols-2">
            <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2">
              <div className="text-[var(--text-muted)]">{t("answerSourceMix")}</div>
              <div className="mt-1 font-mono text-[var(--text-primary)]">
                {formatSourceMix(answerResponse.answer_source_mix)}
              </div>
            </div>
            <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2">
              <div className="text-[var(--text-muted)]">{t("failureClass")}</div>
              <div className="mt-1 font-mono text-[var(--text-primary)]">
                {formatFailureClass(answerResponse.failure_class)}
              </div>
            </div>
          </div>

          {gating.gating_score !== undefined ? (
            <div className="mt-3 grid gap-2 text-xs text-[var(--text-secondary)] md:grid-cols-4">
              <BudgetPill
                label={t("gatingScore")}
                value={`${gating.gating_score}/${gating.gating_threshold ?? "-"}`}
              />
              <BudgetPill label={t("gateProfileLabel")} value={gating.gating_profile ?? "balanced"} />
              <BudgetPill label={t("decisionStage")} value={gating.decision_stage ?? "answered"} />
              <BudgetPill
                label={t("gatingReason")}
                value={gating.gating_hard_block_reason ?? gating.gating_decision_reason ?? "none"}
              />
            </div>
          ) : null}

          {gating.gating_breakdown ? (
            <div className="mt-3 grid gap-2 text-xs text-[var(--text-secondary)] md:grid-cols-4">
              <BudgetPill label="doc" value={gating.gating_breakdown.document_signal} />
              <BudgetPill label="lex" value={gating.gating_breakdown.lexical_grounding} />
              <BudgetPill label="cover" value={gating.gating_breakdown.coverage} />
              <BudgetPill label="chunks" value={gating.gating_breakdown.multi_chunk} />
              <BudgetPill label="sources" value={gating.gating_breakdown.cross_source} />
              <BudgetPill label="lookup" value={gating.gating_breakdown.lookup_boost} />
              <BudgetPill label="dense" value={gating.gating_breakdown.dense_only_penalty} />
              <BudgetPill label="docs" value={gating.gating_breakdown.docs_query_boost} />
            </div>
          ) : null}

          {budget ? (
            <div className="mt-3 grid gap-2 text-xs text-[var(--text-secondary)] md:grid-cols-4">
              <BudgetPill label={t("budgetTotal")} value={budget.token_budget} />
              <BudgetPill label={t("budgetDocuments")} value={budget.used_by_documents} />
              <BudgetPill label={t("budgetMemory")} value={budget.used_by_memory} />
              <BudgetPill label={t("budgetGraph")} value={budget.used_by_graph} />
            </div>
          ) : null}

          {sourceGroups.length > 0 ? (
            <div className="mt-4">
              <div className="mb-2 text-[11px] tracking-[0.12em] text-[var(--text-muted)] uppercase">
                {t("sourceGroupsTitle")}
              </div>
              <div className="flex flex-wrap gap-2">
                {sourceGroups.slice(0, 8).map((group) => (
                  <span
                    key={group.group_id}
                    className="rounded-full border border-[var(--border-subtle)] px-2 py-1 text-[11px] text-[var(--text-secondary)]"
                    title={group.file_paths.join("\n")}
                  >
                    {group.canonical_title} · {group.evidence_count}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          {memories.length > 0 ? (
            <div className="mt-4">
              <div className="mb-2 text-[11px] tracking-[0.12em] text-[var(--text-muted)] uppercase">
                {t("memoryContextTitle")}
              </div>
              <div className="space-y-2">
                {memories.slice(0, 5).map((memory) => (
                  <div key={memory.id} className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-[11px] text-[var(--text-muted)]">
                      <span className="font-mono">#{memory.id}</span>
                      <span>{memory.layer}</span>
                      <span>{memory.scope}</span>
                      <span>{memory.memory_type}</span>
                      <span>{memory.source_type}</span>
                      <span>{Math.round(memory.confidence * 100)}%</span>
                    </div>
                    <div className="mt-1 text-sm text-[var(--text-primary)]">
                      {memory.title || memory.content.slice(0, 80)}
                    </div>
                    <div className="mt-1 line-clamp-2 text-xs text-[var(--text-secondary)]">
                      {memory.content}
                    </div>
                    <div className="mt-1 font-mono text-[10px] text-[var(--text-muted)]">
                      {memory.source_ref}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          <div className="mt-3 text-[11px] text-[var(--text-muted)]">{t("evidenceFirewallNote")}</div>
        </>
      )}
    </section>
  );
}

function BudgetPill({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2">
      <div className="text-[var(--text-muted)]">{label}</div>
      <div className="mt-1 font-mono text-[var(--text-primary)]">{value}</div>
    </div>
  );
}

function formatSourceMix(value: AskResponseStructured["answer_source_mix"]) {
  return value ?? "unknown";
}

function formatFailureClass(value: AskResponseStructured["failure_class"]) {
  return value ?? "none";
}
