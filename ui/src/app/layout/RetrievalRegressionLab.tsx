import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  ChevronLeft,
  Clock,
  FileText,
  Gauge,
  ListChecks,
  Play,
  RefreshCw,
  Search,
  Terminal,
  X,
  XCircle
} from "lucide-react";
import {
  getRetrievalRegressionProgress,
  getRetrievalRegressionRun,
  listRetrievalRegressionReports,
  readRetrievalRegressionReport,
  runRetrievalRegression,
  type RetrievalRegressionCase,
  type RetrievalRegressionProgress,
  type RetrievalRegressionReport,
  type RetrievalRegressionReportEntry,
  type RetrievalRegressionRunState,
  type RunRetrievalRegressionPayload
} from "../api/desktop";

type Mode = RunRetrievalRegressionPayload["mode"];
type Profile = RunRetrievalRegressionPayload["profile"];
type CaseModeFilter = "all" | "answer" | "refuse";
type MainTab = "overview" | "cases" | "detail" | "progress";

type RetrievalRegressionLabProps = {
  open: boolean;
  onClose: () => void;
};

export function RetrievalRegressionLab({ open, onClose }: RetrievalRegressionLabProps) {
  const [reports, setReports] = useState<RetrievalRegressionReportEntry[]>([]);
  const [selectedReportPath, setSelectedReportPath] = useState("");
  const [report, setReport] = useState<RetrievalRegressionReport | null>(null);
  const [loadingReports, setLoadingReports] = useState(false);
  const [loadingReport, setLoadingReport] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("offline_deterministic");
  const [profile, setProfile] = useState<Profile>("core_docs");
  const [caseFilter, setCaseFilter] = useState("");
  const [maxCaseSecs, setMaxCaseSecs] = useState(30);
  const [maxIndexPrepSecs, setMaxIndexPrepSecs] = useState(180);
  const [activeRun, setActiveRun] = useState<RetrievalRegressionRunState | null>(null);
  const [progress, setProgress] = useState<RetrievalRegressionProgress | null>(null);
  const [caseModeFilter, setCaseModeFilter] = useState<CaseModeFilter>("all");
  const [failedOnly, setFailedOnly] = useState(false);
  const [caseSearch, setCaseSearch] = useState("");
  const [selectedCaseId, setSelectedCaseId] = useState("");
  const [tab, setTab] = useState<MainTab>("overview");
  const lastRunStatus = useRef<string | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    void refreshReports();
    void refreshActiveRun();
  }, [open]);

  useEffect(() => {
    if (!selectedReportPath) {
      setReport(null);
      return;
    }
    void loadReport(selectedReportPath);
  }, [selectedReportPath]);

  useEffect(() => {
    if (!activeRun || activeRun.status !== "running") {
      return;
    }
    const timer = window.setInterval(() => {
      void refreshActiveRun(activeRun.id);
      void refreshProgress(activeRun.id);
    }, 1200);
    return () => window.clearInterval(timer);
  }, [activeRun?.id, activeRun?.status]);

  useEffect(() => {
    if (!activeRun) {
      return;
    }
    const previous = lastRunStatus.current;
    lastRunStatus.current = activeRun.status;
    if (activeRun.status === "running") {
      return;
    }
    if (previous === "running") {
      void refreshReports(activeRun.report_path ?? undefined);
      setTab("overview");
    }
  }, [activeRun?.status]);

  const filteredCases = useMemo(() => {
    const query = caseSearch.trim().toLowerCase();
    return (report?.cases ?? []).filter((item) => {
      if (caseModeFilter !== "all" && item.mode !== caseModeFilter) {
        return false;
      }
      if (failedOnly && isCasePassed(item)) {
        return false;
      }
      if (!query) {
        return true;
      }
      return (
        item.id.toLowerCase().includes(query) ||
        item.query.toLowerCase().includes(query) ||
        item.target_documents.some((doc) => doc.toLowerCase().includes(query)) ||
        item.gating_decision_reason.toLowerCase().includes(query)
      );
    });
  }, [caseModeFilter, caseSearch, failedOnly, report?.cases]);

  const selectedCase = useMemo(() => {
    if (!selectedCaseId) {
      return null;
    }
    return (report?.cases ?? []).find((item) => item.id === selectedCaseId) ?? null;
  }, [report?.cases, selectedCaseId]);

  async function refreshReports(preferReportPath?: string) {
    setLoadingReports(true);
    setError(null);
    try {
      const nextReports = await listRetrievalRegressionReports();
      setReports(nextReports);
      setSelectedReportPath((current) => {
        if (preferReportPath && nextReports.some((item) => item.json_path === preferReportPath)) {
          return preferReportPath;
        }
        if (current && nextReports.some((item) => item.json_path === current)) {
          return current;
        }
        return nextReports[0]?.json_path ?? "";
      });
    } catch (err) {
      setError(toMessage(err));
    } finally {
      setLoadingReports(false);
    }
  }

  async function loadReport(path: string) {
    setLoadingReport(true);
    setError(null);
    try {
      const nextReport = await readRetrievalRegressionReport(path);
      setReport(nextReport);
      setSelectedCaseId("");
    } catch (err) {
      setError(toMessage(err));
      setReport(null);
    } finally {
      setLoadingReport(false);
    }
  }

  async function refreshActiveRun(runId?: string) {
    try {
      const run = await getRetrievalRegressionRun(runId);
      setActiveRun(run);
    } catch {
      setActiveRun(null);
    }
  }

  async function refreshProgress(runId?: string) {
    try {
      setProgress(await getRetrievalRegressionProgress(runId));
    } catch {
      setProgress(null);
    }
  }

  async function startRun() {
    setError(null);
    try {
      const run = await runRetrievalRegression({
        mode,
        profile,
        caseFilter: caseFilter.trim() || undefined,
        maxCaseSecs,
        maxIndexPrepSecs
      });
      setActiveRun(run);
      setProgress(null);
      lastRunStatus.current = "running";
      setTab("progress");
    } catch (err) {
      setError(toMessage(err));
    }
  }

  function openCase(id: string) {
    setSelectedCaseId(id);
    setTab("detail");
  }

  const running = activeRun?.status === "running";
  const summary = report?.summary ?? null;

  const tabs: Array<{ id: MainTab; label: string; icon: ReactNode; badge?: ReactNode }> = [
    { id: "overview", label: "总览", icon: <Gauge className="h-3.5 w-3.5" /> },
    {
      id: "cases",
      label: "用例",
      icon: <ListChecks className="h-3.5 w-3.5" />,
      badge: report ? <span className="lab-tab-count">{report.cases.length}</span> : null
    },
    { id: "detail", label: "详情", icon: <FileText className="h-3.5 w-3.5" /> },
    {
      id: "progress",
      label: "进度",
      icon: <Terminal className="h-3.5 w-3.5" />,
      badge: running ? <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-400" /> : null
    }
  ];

  return (
    <div
      className={`absolute inset-0 z-40 flex flex-col overflow-hidden bg-[var(--bg-canvas)] transition-transform duration-300 ${
        open ? "translate-x-0" : "translate-x-full"
      }`}
    >
      <header className="flex shrink-0 items-center justify-between gap-4 border-b border-[var(--border-subtle)] px-5 py-3.5">
        <div className="min-w-0">
          <div className="text-[11px] font-semibold uppercase tracking-[0.18em] text-[var(--text-muted)]">
            检索回归实验室
          </div>
          <h2 className="truncate text-lg font-semibold tracking-tight text-[var(--text-primary)]">
            检索回归控制台
          </h2>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            onClick={() => void refreshReports()}
            className="lab-button"
            disabled={loadingReports}
          >
            <RefreshCw className={`h-4 w-4 ${loadingReports ? "animate-spin" : ""}`} />
            刷新
          </button>
          <button type="button" onClick={onClose} className="lab-icon-button" aria-label="关闭检索回归面板">
            <X className="h-4 w-4" />
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 gap-3 p-3">
        <aside className="flex w-[300px] shrink-0 flex-col gap-4 overflow-hidden border-r border-[var(--border-subtle)] pr-3">
          <section className="shrink-0">
            <SectionLabel>启动任务</SectionLabel>
            <div className="grid gap-2.5">
              <LabField label="模式">
                <select
                  value={mode}
                  onChange={(event) => setMode(event.target.value as Mode)}
                  className="lab-select"
                  disabled={running}
                >
                  <option value="offline_deterministic">离线确定性</option>
                  <option value="live_embedding">在线向量</option>
                </select>
              </LabField>
              <LabField label="配置集">
                <select
                  value={profile}
                  onChange={(event) => setProfile(event.target.value as Profile)}
                  className="lab-select"
                  disabled={running}
                >
                  <option value="core_docs">核心文档</option>
                  <option value="repo_mixed">仓库混合</option>
                  <option value="full_live">全量在线</option>
                </select>
              </LabField>
              <LabField label="用例筛选">
                <input
                  value={caseFilter}
                  onChange={(event) => setCaseFilter(event.target.value)}
                  className="lab-input"
                  placeholder="例如：R42 或 R01,R02"
                  disabled={running}
                />
              </LabField>
              <div className="grid grid-cols-2 gap-2">
                <LabField label="用例超时">
                  <input
                    value={maxCaseSecs}
                    onChange={(event) => setMaxCaseSecs(Number(event.target.value) || 30)}
                    className="lab-input"
                    type="number"
                    min={5}
                    disabled={running}
                  />
                </LabField>
                <LabField label="索引准备">
                  <input
                    value={maxIndexPrepSecs}
                    onChange={(event) => setMaxIndexPrepSecs(Number(event.target.value) || 180)}
                    className="lab-input"
                    type="number"
                    min={10}
                    disabled={running}
                  />
                </LabField>
              </div>
              <button type="button" onClick={() => void startRun()} className="lab-run-button" disabled={running}>
                {running ? <Clock className="h-4 w-4 animate-pulse" /> : <Play className="h-4 w-4" />}
                {running ? "运行中..." : "启动回归"}
              </button>
            </div>
          </section>

          <section className="flex min-h-0 flex-1 flex-col">
            <div className="mb-2 flex shrink-0 items-center justify-between">
              <SectionLabel className="mb-0">报告</SectionLabel>
              <span className="text-[11px] text-[var(--text-muted)]">{reports.length}</span>
            </div>
            <div className="min-h-0 flex-1 space-y-1.5 overflow-y-auto pr-1">
              {reports.map((item) => (
                <button
                  key={item.json_path}
                  type="button"
                  onClick={() => setSelectedReportPath(item.json_path)}
                  className={`w-full overflow-hidden rounded-lg border px-3 py-2 text-left transition ${
                    selectedReportPath === item.json_path
                      ? "border-[var(--accent)] bg-[var(--accent-soft)]"
                      : "border-transparent bg-[var(--bg-surface-2)] hover:border-[var(--border-subtle)]"
                  }`}
                >
                  <div className="flex min-w-0 flex-col items-start gap-1.5">
                    <div className="w-full truncate text-xs font-semibold text-[var(--text-primary)]">{item.name}</div>
                    <div className="flex w-full flex-wrap items-center gap-1.5">
                      <HealthBadge value={item.service_health} />
                      <HealthBadge value={item.rerank_health} label="重排" />
                    </div>
                  </div>
                  <div className="mt-1 flex min-w-0 flex-col gap-1 text-[11px] text-[var(--text-muted)]">
                    <span className="w-full truncate">
                      {modeLabel(item.mode)} / {profileLabel(item.profile)}
                    </span>
                    <span className="w-full truncate">{formatReportTimestamp(item.generated_at_utc)}</span>
                    <div className="flex w-full flex-wrap items-center gap-x-2 gap-y-1 tabular-nums">
                      <span>T1 {formatPct(item.summary.top1_document_hit_rate)}</span>
                      <span>MRR {formatRatio(item.summary.chunk_mrr, 3)}</span>
                    </div>
                  </div>
                </button>
              ))}
              {!loadingReports && reports.length === 0 ? (
                <div className="rounded-lg border border-dashed border-[var(--border-subtle)] p-3 text-xs leading-6 text-[var(--text-secondary)]">
                  还没有报告。先跑一次“离线确定性 / 核心文档”。
                </div>
              ) : null}
            </div>
          </section>
        </aside>

        <main className="flex min-h-0 flex-1 flex-col gap-3">
          {error ? (
            <div className="shrink-0 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-300">
              {error}
            </div>
          ) : null}

          <div className="flex shrink-0 items-center gap-1 self-start rounded-full bg-[var(--bg-surface-2)] p-1">
            {tabs.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => setTab(item.id)}
                className={`flex items-center gap-1.5 rounded-full px-3.5 py-1.5 text-xs font-medium transition ${
                  tab === item.id
                    ? "bg-[var(--accent-soft)] text-[var(--text-primary)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                }`}
              >
                {item.icon}
                {item.label}
                {item.badge}
              </button>
            ))}
          </div>

          <div className="min-h-0 flex-1 overflow-hidden">
            {tab === "overview" ? (
              <OverviewTab
                report={report}
                summary={summary}
                loadingReport={loadingReport}
                progress={progress}
                run={activeRun}
              />
            ) : null}
            {tab === "cases" ? (
              <CasesTab
                report={report}
                filteredCases={filteredCases}
                selectedCaseId={selectedCaseId}
                caseSearch={caseSearch}
                setCaseSearch={setCaseSearch}
                caseModeFilter={caseModeFilter}
                setCaseModeFilter={setCaseModeFilter}
                failedOnly={failedOnly}
                setFailedOnly={setFailedOnly}
                onOpenCase={openCase}
              />
            ) : null}
            {tab === "detail" ? <DetailTab item={selectedCase} onBack={() => setTab("cases")} /> : null}
            {tab === "progress" ? <ProgressTab run={activeRun} progress={progress} /> : null}
          </div>
        </main>
      </div>
    </div>
  );
}

function OverviewTab({
  report,
  summary,
  loadingReport,
  progress,
  run
}: {
  report: RetrievalRegressionReport | null;
  summary: RetrievalRegressionReport["summary"] | null;
  loadingReport: boolean;
  progress: RetrievalRegressionProgress | null;
  run: RetrievalRegressionRunState | null;
}) {
  return (
    <div className="h-full space-y-4 overflow-y-auto pr-1">
      {run?.status === "running" ? <RunningBanner run={run} progress={progress} /> : null}

      <div>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold text-[var(--text-primary)]">
              {report ? `${modeLabel(report.evaluation_mode)} / ${profileLabel(report.profile)}` : "未选择报告"}
            </div>
            <div className="mt-0.5 truncate text-xs text-[var(--text-muted)]">
              {report
                ? `${report.generated_at_utc} | ${report.watch_root}`
                : loadingReport
                  ? "正在加载报告..."
                  : "请选择一份报告，或先启动一次新的回归。"}
            </div>
          </div>
          {report ? (
            <div className="flex flex-wrap items-center gap-2">
              <HealthBadge value={report.service_health} />
              <HealthBadge value={report.rerank_health} label="重排" />
              <span className="lab-chip">用例 {summary?.case_count ?? 0}</span>
              <span className="lab-chip">超时 {report.case_timeout_count}</span>
              <span className="lab-chip">索引 {formatMs(report.index_prep_ms)}</span>
            </div>
          ) : null}
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2.5 sm:grid-cols-3 xl:grid-cols-7">
          <MetricTile label="文档 Top-1" value={formatPct(summary?.top1_document_hit_rate)} />
          <MetricTile label="文档 Top-3" value={formatPct(summary?.top3_document_recall_rate)} />
          <MetricTile label="分块 Top-1" value={formatPct(summary?.top1_chunk_hit_rate)} />
          <MetricTile label="分块 Top-5" value={formatPct(summary?.top5_chunk_recall_rate)} />
          <MetricTile label="分块 MRR" value={formatRatio(summary?.chunk_mrr, 4)} />
          <MetricTile label="应用重排" value={formatPct(summary?.rerank_applied_rate)} />
          <MetricTile label="引用有效" value={formatPct(summary?.citation_validity_rate)} />
        </div>

        <div className="mt-2 grid grid-cols-2 gap-2.5 sm:grid-cols-3 xl:grid-cols-4">
          <MetricTile label="拒答正确率" value={formatPct(summary?.reject_correctness_rate)} />
          <MetricTile label="回答用例" value={String(summary?.answer_cases ?? 0)} />
          <MetricTile label="拒答用例" value={String(summary?.refuse_cases ?? 0)} />
          <MetricTile label="在线服务" value={report?.live_service_used ? "是" : "否"} />
        </div>

        {report?.preparation_error ? (
          <div className="mt-3 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-300">
            准备阶段错误：{report.preparation_error}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function CasesTab({
  report,
  filteredCases,
  selectedCaseId,
  caseSearch,
  setCaseSearch,
  caseModeFilter,
  setCaseModeFilter,
  failedOnly,
  setFailedOnly,
  onOpenCase
}: {
  report: RetrievalRegressionReport | null;
  filteredCases: RetrievalRegressionCase[];
  selectedCaseId: string;
  caseSearch: string;
  setCaseSearch: (value: string) => void;
  caseModeFilter: CaseModeFilter;
  setCaseModeFilter: (value: CaseModeFilter) => void;
  failedOnly: boolean;
  setFailedOnly: (value: boolean) => void;
  onOpenCase: (id: string) => void;
}) {
  if (!report) {
    return <EmptyHint>请先选择一份报告。</EmptyHint>;
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <div className="relative">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--text-muted)]" />
            <input
              value={caseSearch}
              onChange={(event) => setCaseSearch(event.target.value)}
              className="lab-input !w-72 pl-8"
              placeholder="搜索用例、查询、文档或门控原因"
            />
          </div>
          <select
            value={caseModeFilter}
            onChange={(event) => setCaseModeFilter(event.target.value as CaseModeFilter)}
            className="lab-select !w-28"
          >
            <option value="all">全部</option>
            <option value="answer">回答</option>
            <option value="refuse">拒答</option>
          </select>
        </div>
        <label className="flex cursor-pointer items-center gap-2 text-xs text-[var(--text-secondary)]">
          <input
            type="checkbox"
            checked={failedOnly}
            onChange={(event) => setFailedOnly(event.target.checked)}
            className="accent-[var(--accent)]"
          />
          仅看失败
        </label>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto rounded-lg border border-[var(--border-subtle)]">
        <table className="w-full table-fixed border-collapse text-left text-xs">
          <colgroup>
            <col className="w-[72px]" />
            <col className="w-[76px]" />
            <col />
            <col className="w-[56px]" />
            <col className="w-[64px]" />
            <col className="w-[140px]" />
            <col className="w-[74px]" />
            <col className="w-[78px]" />
          </colgroup>
          <thead className="sticky top-0 z-10 bg-[var(--bg-surface-2)] text-[var(--text-muted)]">
            <tr>
              <th className="px-3 py-2 font-medium">用例</th>
              <th className="px-2 py-2 font-medium">模式</th>
              <th className="px-2 py-2 font-medium">查询</th>
              <th className="px-1 py-2 text-center font-medium">文档</th>
              <th className="px-1 py-2 text-center font-medium">分块</th>
              <th className="px-2 py-2 font-medium">门控</th>
              <th className="px-1 py-2 text-center font-medium">重排</th>
              <th className="px-2 py-2 text-right font-medium">总耗时</th>
            </tr>
          </thead>
          <tbody>
            {filteredCases.map((item) => {
              const passed = isCasePassed(item);
              return (
                <tr
                  key={item.id}
                  onClick={() => onOpenCase(item.id)}
                  className={`cursor-pointer border-t border-[var(--border-subtle)] transition hover:bg-[var(--bg-surface-2)] ${
                    selectedCaseId === item.id ? "bg-[var(--accent-soft)]" : ""
                  }`}
                >
                  <td className="px-3 py-2">
                    <div className="flex items-center gap-1.5 font-semibold text-[var(--text-primary)]">
                      {passed ? (
                        <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-emerald-400" />
                      ) : (
                        <XCircle className="h-3.5 w-3.5 shrink-0 text-red-400" />
                      )}
                      {item.id}
                    </div>
                  </td>
                  <td className="px-2 py-2 text-[var(--text-secondary)]">{caseModeLabel(item.mode)}</td>
                  <td className="truncate px-2 py-2 text-[var(--text-secondary)]">{item.query}</td>
                  <td className="px-1 py-2 text-center text-[var(--text-secondary)]">{rankLabel(item.document_hit_rank)}</td>
                  <td className="px-1 py-2 text-center text-[var(--text-secondary)]">{rankLabel(item.chunk_hit_rank)}</td>
                  <td className="truncate px-2 py-2 text-[var(--text-secondary)]" title={item.gating_decision_reason}>
                    {gatingReasonLabel(item.gating_decision_reason)}
                  </td>
                  <td className="px-1 py-2 text-center">
                    <ResultPill ok={item.rerank_applied} trueLabel="是" falseLabel="否" />
                  </td>
                  <td className="px-2 py-2 text-right tabular-nums text-[var(--text-secondary)]">
                    {formatMs(caseTotalMs(item))}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {filteredCases.length === 0 ? (
          <div className="p-6 text-center text-sm text-[var(--text-muted)]">没有匹配的用例。</div>
        ) : null}
      </div>
    </div>
  );
}

function DetailTab({ item, onBack }: { item: RetrievalRegressionCase | null; onBack: () => void }) {
  if (!item) {
    return <EmptyHint>请从列表中选择一个用例查看详情。</EmptyHint>;
  }

  const failedReasons = getFailedReasons(item);
  return (
    <div className="h-full space-y-4 overflow-y-auto pr-1">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-base font-semibold text-[var(--text-primary)]">{item.id}</span>
            {isCasePassed(item) ? (
              <span className="rounded-full bg-emerald-500/12 px-2 py-0.5 text-[11px] font-semibold text-emerald-300">
                通过
              </span>
            ) : (
              <span className="rounded-full bg-red-500/12 px-2 py-0.5 text-[11px] font-semibold text-red-300">
                失败
              </span>
            )}
          </div>
          <div className="mt-0.5 text-xs text-[var(--text-muted)]">
            {caseModeLabel(item.mode)} | {runStatusLabel(item.status)}
          </div>
        </div>
        <button type="button" onClick={onBack} className="lab-button shrink-0">
          <ChevronLeft className="h-4 w-4" />
          返回用例
        </button>
      </div>

      <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2.5">
        <SectionLabel>查询</SectionLabel>
        <div className="text-sm leading-6 text-[var(--text-primary)]">{item.query}</div>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <DetailSection title="失败信号">
          {failedReasons.length === 0 ? (
            <div className="text-xs text-emerald-300">未检测到失败信号。</div>
          ) : (
            failedReasons.map((reason) => (
              <div key={reason} className="rounded-md bg-red-500/10 px-3 py-2 text-xs text-red-300">
                {reason}
              </div>
            ))
          )}
        </DetailSection>

        <DetailSection title="指标">
          <div className="grid grid-cols-3 gap-2">
            <MiniMetric label="文档排名" value={rankLabel(item.document_hit_rank)} />
            <MiniMetric label="分块排名" value={rankLabel(item.chunk_hit_rank)} />
            <MiniMetric label="分块 Top-1" value={yesNo(item.top1_chunk_hit)} />
            <MiniMetric label="引用数" value={String(item.citations_count)} />
            <MiniMetric label="证据数" value={String(item.final_evidence_count)} />
            <MiniMetric label="重排" value={item.rerank_applied ? "已应用" : "否"} />
            <MiniMetric label="文档召回" value={formatMs(item.doc_recall_ms)} />
            <MiniMetric label="文档向量" value={formatMs(item.doc_dense_ms)} />
            <MiniMetric label="分块词法" value={formatMs(item.chunk_lexical_ms)} />
            <MiniMetric label="分块向量" value={formatMs(item.chunk_dense_ms)} />
            <MiniMetric label="重排耗时" value={formatMs(item.rerank_ms)} />
            <MiniMetric label="合并耗时" value={formatMs(item.merge_ms)} />
          </div>
        </DetailSection>

        <DetailSection title="门控">
          <div className="rounded-md bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
            {gatingReasonLabel(item.gating_decision_reason)}
          </div>
        </DetailSection>

        <DetailSection title="目标文档">
          {item.target_documents.length === 0 ? (
            <div className="text-xs text-[var(--text-muted)]">没有目标文档。</div>
          ) : (
            item.target_documents.map((doc) => (
              <div key={doc} className="break-all rounded-md bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                {doc}
              </div>
            ))
          )}
        </DetailSection>

        <DetailSection title="目标线索">
          {item.target_clues.length === 0 ? (
            <div className="text-xs text-[var(--text-muted)]">没有目标线索。</div>
          ) : (
            item.target_clues.map((clue) => (
              <div key={clue} className="break-all rounded-md bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                {clue}
              </div>
            ))
          )}
        </DetailSection>

        {item.notes ? (
          <DetailSection title="备注">
            <div className="rounded-md bg-[var(--bg-surface-2)] px-3 py-2 text-xs leading-5 text-[var(--text-secondary)]">
              {item.notes}
            </div>
          </DetailSection>
        ) : null}
      </div>
    </div>
  );
}

function ProgressTab({
  run,
  progress
}: {
  run: RetrievalRegressionRunState | null;
  progress: RetrievalRegressionProgress | null;
}) {
  if (!run) {
    return <EmptyHint>尚未启动任何回归任务。</EmptyHint>;
  }

  const running = run.status === "running";
  const total = progress?.total ?? 0;
  const completed = progress?.completed ?? 0;
  const remaining = Math.max(total - completed, 0);
  const pct = total > 0 ? Math.min(100, Math.round((completed / total) * 100)) : 0;

  return (
    <div className="flex h-full min-h-0 flex-col gap-4">
      <div className="shrink-0 space-y-4">
        <RunStatus run={run} />

        {progress && total > 0 ? (
          <div>
            <div className="flex items-center justify-between text-xs text-[var(--text-muted)]">
              <span>
                进度 {completed} / {total}
              </span>
              <span>剩余 {remaining}</span>
            </div>
            <div className="mt-2 h-2 overflow-hidden rounded-full bg-[var(--bg-surface-2)]">
              <div
                className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-500"
                style={{ width: `${pct}%` }}
              />
            </div>
            <div className="mt-3 grid grid-cols-4 gap-2">
              <MiniMetric label="剩余" value={String(remaining)} />
              <MiniMetric label="完成" value={String(completed)} />
              <MiniMetric label="通过" value={String(progress.passed)} />
              <MiniMetric label="失败" value={String(progress.failed)} />
            </div>
          </div>
        ) : running ? (
          <div className="rounded-lg border border-dashed border-[var(--border-subtle)] px-3 py-2 text-xs leading-5 text-[var(--text-secondary)]">
            还在等待结构化的逐用例进度。下面的 stdout / stderr 仍然是实时输出。
          </div>
        ) : null}

        {progress && progress.current_case_id ? (
          <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2 text-xs font-semibold text-[var(--text-primary)]">
                <span className="rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent)]">
                  {progress.current_index > 0 ? `#${progress.current_index}` : "--"} {progress.current_case_id}
                </span>
                {progress.current_mode ? (
                  <span className="text-[var(--text-muted)]">{caseModeLabel(progress.current_mode)}</span>
                ) : null}
              </div>
              <PhaseBadge phase={progress.current_phase} running={running} />
            </div>
            <SectionLabel className="mt-2">当前查询</SectionLabel>
            <div className="text-sm leading-6 text-[var(--text-primary)]">
              {progress.current_query || "（暂无查询文本）"}
            </div>
          </div>
        ) : null}
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        <SectionLabel>运行输出</SectionLabel>
        <div className="min-h-0 flex-1 overflow-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] p-3">
          {run.stdout_tail || run.stderr_tail ? (
            <pre className="whitespace-pre-wrap break-words font-mono text-[11px] leading-5 text-[var(--text-secondary)]">
              {run.stdout_tail}
              {run.stderr_tail ? `\n--- 标准错误 stderr ---\n${run.stderr_tail}` : ""}
            </pre>
          ) : (
            <div className="text-xs text-[var(--text-muted)]">
              {running ? "正在等待实时进程输出..." : "暂无日志输出。"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function RunningBanner({
  run,
  progress
}: {
  run: RetrievalRegressionRunState;
  progress: RetrievalRegressionProgress | null;
}) {
  const total = progress?.total ?? 0;
  const completed = progress?.completed ?? 0;
  const remaining = Math.max(total - completed, 0);
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2.5 text-sm text-amber-200">
      <div className="flex items-center gap-2">
        <Clock className="h-4 w-4 animate-pulse" />
        <span className="font-semibold">回归任务运行中</span>
        <span className="text-amber-200/80">
          {modeLabel(run.mode)} / {profileLabel(run.profile)}
        </span>
      </div>
      {total > 0 ? (
        <div className="flex items-center gap-3 text-xs text-amber-200/90">
          {progress?.current_case_id ? <span>当前 {progress.current_case_id}</span> : null}
          <span>
            {completed} / {total}（剩余 {remaining}）
          </span>
        </div>
      ) : null}
    </div>
  );
}

function PhaseBadge({ phase, running }: { phase: string; running: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full bg-[var(--bg-surface-1)] px-2.5 py-1 text-[11px] font-semibold text-[var(--text-secondary)]">
      {running ? <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-[var(--accent)]" /> : null}
      {phaseLabel(phase)}
    </span>
  );
}

function SectionLabel({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <div className={`mb-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-[var(--text-muted)] ${className}`}>
      {children}
    </div>
  );
}

function EmptyHint({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full items-center justify-center rounded-lg border border-dashed border-[var(--border-subtle)] px-6 text-center text-sm text-[var(--text-muted)]">
      {children}
    </div>
  );
}

function LabField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-[11px] font-semibold uppercase tracking-[0.12em] text-[var(--text-muted)]">
        {label}
      </span>
      {children}
    </label>
  );
}

function RunStatus({ run }: { run: RetrievalRegressionRunState }) {
  const tone =
    run.status === "running"
      ? "text-amber-300"
      : run.status === "succeeded"
        ? "text-emerald-300"
        : "text-red-300";
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className={`flex items-center gap-2 text-sm font-semibold ${tone}`}>
        {run.status === "running" ? (
          <Clock className="h-4 w-4 animate-pulse" />
        ) : run.status === "succeeded" ? (
          <CheckCircle2 className="h-4 w-4" />
      ) : (
        <AlertTriangle className="h-4 w-4" />
      )}
        {runStatusLabel(run.status)}
        <span className="text-[var(--text-muted)]">
          | {modeLabel(run.mode)} / {profileLabel(run.profile)}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-[11px] text-[var(--text-muted)]">
        {run.case_filter ? <span className="lab-chip">用例：{run.case_filter}</span> : null}
        {run.exit_code !== null ? <span className="lab-chip">退出码：{run.exit_code}</span> : null}
      </div>
      {run.error ? <div className="w-full text-xs text-red-300">{run.error}</div> : null}
    </div>
  );
}

function MetricTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2.5">
      <div className="truncate text-[11px] font-medium text-[var(--text-muted)]">{label}</div>
      <div className="mt-1 text-lg font-semibold tabular-nums text-[var(--text-primary)]">{value}</div>
    </div>
  );
}

function MiniMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md bg-[var(--bg-surface-1)] px-2 py-1.5 text-center">
      <div className="text-[10px] text-[var(--text-muted)]">{label}</div>
      <div className="truncate text-[11px] font-semibold tabular-nums text-[var(--text-primary)]">{value}</div>
    </div>
  );
}

function HealthBadge({ value, label }: { value: string; label?: string }) {
  const normalized = value || "unknown";
  const tone =
    normalized === "ready"
      ? "bg-emerald-500/12 text-emerald-300"
      : normalized === "degraded"
        ? "bg-amber-500/12 text-amber-300"
        : normalized === "disabled"
          ? "bg-slate-500/12 text-slate-300"
          : "bg-red-500/12 text-red-300";
  return (
    <span className={`inline-flex max-w-full min-w-0 items-center rounded-full px-2 py-0.5 text-[11px] font-semibold ${tone}`}>
      <span className="truncate">{label ? `${label}：${healthLabel(normalized)}` : healthLabel(normalized)}</span>
    </span>
  );
}

function ResultPill({
  ok,
  trueLabel = "ok",
  falseLabel = "miss"
}: {
  ok: boolean;
  trueLabel?: string;
  falseLabel?: string;
}) {
  return (
    <span
      className={`inline-block rounded-full px-1.5 py-0.5 text-[10px] font-semibold ${
        ok ? "bg-emerald-500/12 text-emerald-300" : "bg-red-500/12 text-red-300"
      }`}
    >
      {ok ? trueLabel : falseLabel}
    </span>
  );
}

function DetailSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <SectionLabel>{title}</SectionLabel>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

function isCasePassed(item: RetrievalRegressionCase) {
  if (item.timed_out) {
    return false;
  }
  if (item.mode === "refuse") {
    return item.reject_correct;
  }
  return item.top3_document_recall && item.top5_chunk_recall && item.citation_valid;
}

function getFailedReasons(item: RetrievalRegressionCase) {
  const reasons: string[] = [];
  if (item.timed_out) {
    reasons.push("用例执行超时");
  }
  if (item.mode === "refuse") {
    if (!item.reject_correct) {
      reasons.push("该拒答但没有正确拒答");
    }
    return reasons;
  }
  if (!item.top1_document_hit) {
    reasons.push("目标文档未排到 Top-1");
  }
  if (!item.top3_document_recall) {
    reasons.push("目标文档未进入 Top-3");
  }
  if (!item.top5_chunk_recall) {
    reasons.push("目标线索未进入 Top-5 分块");
  }
  if (!item.citation_valid) {
    reasons.push("引用校验失败");
  }
  if (item.gating_decision_reason) {
    reasons.push(`门控：${gatingReasonLabel(item.gating_decision_reason)}`);
  }
  return reasons;
}

function phaseLabel(phase: string) {
  switch (phase) {
    case "preparing":
      return "准备中";
    case "indexing":
      return "索引中";
    case "doc_recall":
      return "文档召回";
    case "chunk_recall":
      return "分块召回";
    case "rerank":
      return "重排";
    case "gating":
      return "门控";
    case "scoring":
      return "评分";
    case "done":
      return "完成";
    case "":
      return "运行中";
    default:
      return phase;
  }
}

function formatPct(value: number | null | undefined) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "--";
  }
  return `${(value * 100).toFixed(1)}%`;
}

function formatRatio(value: number | null | undefined, digits = 3) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "--";
  }
  return value.toFixed(digits);
}

function formatMs(value: number | null | undefined) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "--";
  }
  if (value >= 1000) {
    return `${(value / 1000).toFixed(1)}s`;
  }
  return `${value}ms`;
}

function formatReportTimestamp(value: string | null | undefined) {
  if (!value) {
    return "--";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false
  }).format(date);
}

function rankLabel(value: number | null) {
  return value === null ? "-" : `#${value}`;
}

function caseTotalMs(item: RetrievalRegressionCase) {
  return (
    item.doc_recall_ms +
    item.doc_dense_ms +
    item.chunk_lexical_ms +
    item.chunk_dense_ms +
    item.merge_ms +
    item.rerank_ms
  );
}

function yesNo(value: boolean) {
  return value ? "是" : "否";
}

function toMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

function modeLabel(value: string) {
  switch (value) {
    case "offline_deterministic":
      return "离线确定性";
    case "live_embedding":
      return "在线向量";
    default:
      return value;
  }
}

function profileLabel(value: string) {
  switch (value) {
    case "core_docs":
      return "核心文档";
    case "repo_mixed":
      return "仓库混合";
    case "full_live":
      return "全量在线";
    default:
      return value;
  }
}

function caseModeLabel(value: string) {
  switch (value) {
    case "all":
      return "全部";
    case "answer":
      return "回答";
    case "refuse":
      return "拒答";
    default:
      return value;
  }
}

function runStatusLabel(value: string) {
  switch (value) {
    case "running":
      return "运行中";
    case "succeeded":
      return "成功";
    case "failed":
      return "失败";
    case "idle":
      return "空闲";
    default:
      return value;
  }
}

function healthLabel(value: string) {
  switch (value) {
    case "ready":
      return "正常";
    case "degraded":
      return "降级";
    case "disabled":
      return "未启用";
    case "unavailable":
      return "不可用";
    case "unknown":
      return "未知";
    default:
      return value;
  }
}

function gatingReasonLabel(value: string | null | undefined) {
  switch (value || "") {
    case "":
      return "-";
    case "timeout":
      return "超时";
    case "coverage_release":
      return "覆盖度放行";
    case "high_coverage_lexical_release":
      return "高覆盖词法放行";
    case "docs_family_multi_chunk_release":
      return "文档类多分块放行";
    case "semantic_context_release":
      return "语义上下文放行";
    case "insufficient_evidence":
      return "证据不足";
    default:
      return value || "-";
  }
}
