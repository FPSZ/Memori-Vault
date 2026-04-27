import { useEffect, useMemo, useRef, useState } from "react";
import { motion, type Variants } from "framer-motion";
import { Download, FileText, RefreshCw, ScrollText } from "lucide-react";
import { getLogs, getLogDir } from "../../../app/api/desktop";
import type { LogEntry } from "../../../app/api/desktop";
import { useI18n } from "../../../i18n";

const fadeSlideUpVariants: Variants = {
  hidden: { opacity: 0, y: 12 },
  show: { opacity: 1, y: 0, transition: { duration: 0.22, ease: "easeOut" } },
  exit: { opacity: 0, y: -8, transition: { duration: 0.15 } }
};

const staggerContainerVariants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.04 } }
};

const LEVEL_COLORS: Record<string, string> = {
  TRACE: "text-[var(--text-muted)]",
  DEBUG: "text-blue-400",
  INFO: "text-emerald-400",
  WARN: "text-amber-400",
  ERROR: "text-red-400"
};

const LEVEL_BG: Record<string, string> = {
  TRACE: "bg-[var(--text-muted)]/10",
  DEBUG: "bg-blue-400/10",
  INFO: "bg-emerald-400/10",
  WARN: "bg-amber-400/10",
  ERROR: "bg-red-400/10"
};

export function LogsTab() {
  const { t } = useI18n();
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [levelFilter, setLevelFilter] = useState<string>("ALL");
  const [logDir, setLogDir] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const timerRef = useRef<number | null>(null);

  const fetchLogs = async () => {
    setLoading(true);
    try {
      const filter = levelFilter === "ALL" ? null : levelFilter;
      const data = await getLogs({ limit: 500, level_filter: filter });
      setEntries(data);
    } catch {
      // silently ignore
    } finally {
      setLoading(false);
    }
  };

  const fetchLogDir = async () => {
    try {
      const dir = await getLogDir();
      setLogDir(dir);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    void fetchLogs();
    void fetchLogDir();
  }, [levelFilter]);

  useEffect(() => {
    if (autoRefresh) {
      timerRef.current = window.setInterval(() => {
        void fetchLogs();
      }, 3000);
    }
    return () => {
      if (timerRef.current !== null) {
        window.clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [autoRefresh, levelFilter]);

  const filtered = useMemo(() => entries, [entries]);

  const levels = ["ALL", "TRACE", "DEBUG", "INFO", "WARN", "ERROR"];

  const handleExport = () => {
    if (entries.length === 0) return;
    const lines = entries.map((e) => {
      const ts = e.timestamp || "";
      const level = e.level.toUpperCase().padEnd(5);
      const target = e.target || "";
      const msg = e.message || "";
      const loc = e.file && e.line ? ` (${e.file}:${e.line})` : "";
      return `[${ts}] [${level}] [${target}] ${msg}${loc}`;
    });
    const blob = new Blob([lines.join("\n")], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `memori-logs-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.txt`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <motion.div
      key="settings-tab-logs"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
        日志
      </h3>

      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4 pb-2">
        {/* Toolbar */}
        <div className="flex flex-wrap items-center gap-2">
          {levels.map((lv) => (
            <button
              key={lv}
              type="button"
              onClick={() => setLevelFilter(lv)}
              className={`rounded-md px-2.5 py-1 text-xs font-medium transition ${
                levelFilter === lv
                  ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--bg-surface-2)] hover:text-[var(--text-primary)]"
              }`}
            >
              {lv}
            </button>
          ))}
          <div className="ml-auto flex items-center gap-2">
            <label className="flex cursor-pointer items-center gap-1.5 text-xs text-[var(--text-secondary)]">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(e) => setAutoRefresh(e.target.checked)}
                className="h-3.5 w-3.5 accent-[var(--accent)]"
              />
              自动刷新
            </label>
            <button
              type="button"
              onClick={() => void handleExport()}
              className="inline-flex items-center gap-1 rounded-md bg-[var(--bg-surface-2)] px-2.5 py-1 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
            >
              <Download className="h-3 w-3" />
              导出
            </button>
            <button
              type="button"
              onClick={() => void fetchLogs()}
              disabled={loading}
              className="inline-flex items-center gap-1 rounded-md bg-[var(--bg-surface-2)] px-2.5 py-1 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)] disabled:opacity-50"
            >
              <RefreshCw className={`h-3 w-3 ${loading ? "animate-spin" : ""}`} />
              刷新
            </button>
          </div>
        </div>

        {/* Log dir hint */}
        {logDir && (
          <div className="flex items-center gap-1.5 text-[11px] text-[var(--text-muted)]">
            <FileText className="h-3 w-3" />
            <span className="font-mono">{logDir}</span>
          </div>
        )}

        {/* Log table */}
        <div
          ref={scrollRef}
          className="settings-scrollbar h-[520px] overflow-y-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)]"
        >
          {filtered.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-[var(--text-muted)]">
              <ScrollText className="h-8 w-8 opacity-40" />
              <span className="text-sm">暂无日志</span>
            </div>
          ) : (
            <table className="w-full text-left text-xs">
              <thead className="sticky top-0 z-10 bg-[var(--bg-surface-2)]">
                <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
                  <th className="px-3 py-2 font-medium">时间</th>
                  <th className="px-3 py-2 font-medium w-[72px]">级别</th>
                  <th className="px-3 py-2 font-medium">来源</th>
                  <th className="px-3 py-2 font-medium">消息</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[var(--border-subtle)]">
                {filtered.map((entry, idx) => {
                  const level = entry.level.toUpperCase();
                  const colorClass = LEVEL_COLORS[level] ?? "text-[var(--text-secondary)]";
                  const bgClass = LEVEL_BG[level] ?? "";
                  return (
                    <tr key={idx} className="transition hover:bg-[var(--bg-surface-2)]/50">
                      <td className="px-3 py-1.5 whitespace-nowrap font-mono text-[var(--text-muted)]">
                        {(() => {
                          try {
                            const d = new Date(entry.timestamp);
                            const date = d.toLocaleDateString("zh-CN", {
                              month: "2-digit",
                              day: "2-digit"
                            });
                            const time = d.toLocaleTimeString("zh-CN", {
                              hour: "2-digit",
                              minute: "2-digit",
                              second: "2-digit",
                              hour12: false
                            });
                            return `${date} ${time}`;
                          } catch {
                            return entry.timestamp;
                          }
                        })()}
                      </td>
                      <td className="px-3 py-1.5">
                        <span className={`inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase ${colorClass} ${bgClass}`}>
                          {level}
                        </span>
                      </td>
                      <td className="px-3 py-1.5 whitespace-nowrap text-[var(--text-secondary)]" title={entry.target}>
                        {entry.target.split("::").pop() ?? entry.target}
                      </td>
                      <td className="px-3 py-1.5 text-[var(--text-primary)]">
                        <div className="max-w-md truncate" title={entry.message}>
                          {entry.message}
                        </div>
                        {(entry.file || entry.line) && (
                          <div className="mt-0.5 text-[10px] text-[var(--text-muted)]">
                            {entry.file ?? ""}:{entry.line ?? ""}
                          </div>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </motion.div>
    </motion.div>
  );
}
