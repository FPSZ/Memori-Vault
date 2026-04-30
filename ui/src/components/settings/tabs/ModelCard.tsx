import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, ChevronUp, FolderOpen, LoaderCircle, Play, RefreshCw, Square, Zap } from "lucide-react";
import type { LocalModelRuntimeStatusDto } from "../types";
import { extractPort, replacePort, ROLE_META, type ModelRoleKey } from "./models-helpers";

type ModelCardProps = {
  role: ModelRoleKey;
  endpoint: string;
  model: string;
  modelPath?: string | null;
  contextLength?: number | null;
  concurrency?: number | null;
  runtimeStatus?: LocalModelRuntimeStatusDto | null;
  isLocal: boolean;
  busy: boolean;
  runtimeBusy: boolean;
  expanded: boolean;
  validationMessage?: string | null;
  onEndpointChange: (v: string) => void;
  onModelChange: (v: string) => void;
  onContextLengthChange: (v: number | null) => void;
  onConcurrencyChange: (v: number | null) => void;
  onPickFile: () => void;
  onProbe: () => void;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onExpandedChange: (expanded: boolean) => void;
};

export function ModelCard({
  role,
  endpoint,
  model,
  modelPath,
  contextLength,
  concurrency,
  runtimeStatus,
  isLocal,
  busy,
  runtimeBusy,
  expanded,
  validationMessage,
  onEndpointChange,
  onModelChange,
  onContextLengthChange,
  onConcurrencyChange,
  onPickFile,
  onProbe,
  onStart,
  onStop,
  onRestart,
  onExpandedChange
}: ModelCardProps) {
  const meta = ROLE_META[role];
  const Icon = meta.icon;
  const port = extractPort(endpoint);
  const running = runtimeStatus?.state === "running";
  const external = runtimeStatus?.state === "external";
  const starting = runtimeStatus?.state === "starting";
  const processAlive = running || external || starting;
  const stateLabel =
    runtimeStatus?.state === "running"
      ? "运行中"
      : runtimeStatus?.state === "external"
        ? "外部运行"
        : runtimeStatus?.state === "starting"
          ? "加载中"
          : runtimeStatus?.state === "error"
            ? "异常"
            : "未启动";

  return (
    <div className={`overflow-hidden rounded-xl border bg-[var(--bg-surface-1)] ${validationMessage ? "border-red-400/60" : "border-[var(--border-subtle)]"}`}>
      <div className="flex items-center gap-3 px-4 py-3">
        <div className={`flex h-8 w-8 items-center justify-center rounded-lg bg-[var(--bg-surface-2)] ${meta.color}`}>
          <Icon className="h-4 w-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-[var(--text-primary)]">{meta.label}</span>
            {model ? <span className="truncate text-xs text-[var(--text-secondary)]">{model}</span> : null}
          </div>
          <div className="flex flex-wrap items-center gap-2 text-[11px] text-[var(--text-muted)]">
            {port ? <span className="font-mono">端口 {port}</span> : null}
            {isLocal ? (
              <span className={running || external ? "text-emerald-400" : starting ? "text-amber-400" : runtimeStatus?.state === "error" ? "text-red-400" : ""}>
                {stateLabel}{runtimeStatus?.pid ? ` · PID ${runtimeStatus.pid}` : ""}
              </span>
            ) : null}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          {isLocal ? (
            processAlive ? (
              <button
                type="button"
                onClick={onStop}
                disabled={runtimeBusy || external}
                title={external ? "该模型服务不是当前软件会话启动的，请到原进程或系统任务管理器中关闭。" : undefined}
                className="inline-flex items-center gap-1 rounded-md bg-red-500/10 px-2.5 py-1 text-xs font-medium text-red-400 transition hover:opacity-80 disabled:opacity-50"
              >
                {runtimeBusy ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Square className="h-3 w-3" />}
                停止
              </button>
            ) : (
              <button
                type="button"
                onClick={onStart}
                disabled={runtimeBusy}
                className="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 px-2.5 py-1 text-xs font-medium text-emerald-400 transition hover:opacity-80 disabled:opacity-50"
              >
                {runtimeBusy ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Play className="h-3 w-3" />}
                启动
              </button>
            )
          ) : null}
          <button
            type="button"
            onClick={onProbe}
            disabled={busy}
            className="inline-flex items-center gap-1 rounded-md bg-[var(--accent-soft)] px-2.5 py-1 text-xs font-medium text-[var(--accent)] transition hover:opacity-80 disabled:opacity-50"
          >
            {busy ? <LoaderCircle className="h-3 w-3 animate-spin" /> : <Zap className="h-3 w-3" />}
            探测
          </button>
          <button
            type="button"
            onClick={() => onExpandedChange(!expanded)}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-2)] hover:text-[var(--text-primary)]"
          >
            {expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </button>
        </div>
      </div>

      <AnimatePresence>
        {expanded ? (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="space-y-3 border-t border-[var(--border-subtle)] px-4 py-3">
              {validationMessage ? (
                <div className="rounded-lg border border-red-400/30 bg-red-500/10 px-3 py-2 text-xs text-red-400">
                  {validationMessage}
                </div>
              ) : null}

              <div className="space-y-1">
                <label className="text-[11px] font-medium text-[var(--text-muted)]">模型名称</label>
                <input
                  type="text"
                  value={model}
                  onChange={(e) => onModelChange(e.target.value)}
                  className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                  placeholder={meta.defaultModel}
                />
              </div>

              {isLocal ? (
                <div className="space-y-1">
                  <label className="text-[11px] font-medium text-[var(--text-muted)]">GGUF 文件</label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={modelPath ?? ""}
                      readOnly
                      className="min-w-0 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 font-mono text-xs text-[var(--text-primary)] outline-none"
                      placeholder="选择 .gguf 模型文件"
                    />
                    <button
                      type="button"
                      onClick={onPickFile}
                      className="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
                    >
                      <FolderOpen className="h-3.5 w-3.5" />
                      浏览
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="space-y-1">
                <label className="text-[11px] font-medium text-[var(--text-muted)]">端口号</label>
                <input
                  type="text"
                  value={port}
                  onChange={(e) => onEndpointChange(replacePort(endpoint, e.target.value))}
                  className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 font-mono text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                  placeholder={meta.defaultPort}
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1">
                  <label className="text-[11px] font-medium text-[var(--text-muted)]">上下文长度</label>
                  <input
                    type="number"
                    value={contextLength ?? ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      onContextLengthChange(v === "" ? null : Math.max(1, Number(v)));
                    }}
                    className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                    placeholder={role === "chat" ? "16384" : role === "graph" ? "4096" : "8192"}
                  />
                </div>
                <div className="space-y-1">
                  <label className="text-[11px] font-medium text-[var(--text-muted)]">并发数</label>
                  <input
                    type="number"
                    value={concurrency ?? ""}
                    onChange={(e) => {
                      const v = e.target.value;
                      onConcurrencyChange(v === "" ? null : Math.max(1, Number(v)));
                    }}
                    className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                    placeholder="1"
                  />
                </div>
              </div>

              {isLocal && runtimeStatus?.message ? (
                <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2 text-[11px] text-[var(--text-muted)]">
                  {runtimeStatus.message}
                </div>
              ) : null}

              {isLocal && processAlive ? (
                <button
                  type="button"
                  onClick={onRestart}
                  disabled={runtimeBusy}
                  className="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)] disabled:opacity-50"
                >
                  {runtimeBusy ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
                  重启当前模型服务
                </button>
              ) : null}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}
