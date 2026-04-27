import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Bot,
  ChevronDown,
  ChevronUp,
  FolderOpen,
  LoaderCircle,
  MessageSquare,
  Network,
  RefreshCw,
  Save,
  Share2,
  Zap
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatedPressButton } from "../../MotionKit";
import type {
  LocalModelProfileDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto,
  RemoteModelProfileDto
} from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function extractPort(endpoint: string): string {
  try {
    const url = new URL(endpoint);
    return url.port || (url.protocol === "https:" ? "443" : "80");
  } catch {
    return "";
  }
}

function replacePort(endpoint: string, port: string): string {
  try {
    const url = new URL(endpoint);
    url.port = port;
    return url.toString().replace(/\/$/, "");
  } catch {
    return endpoint;
  }
}

function pickModelFile(): Promise<string | null> {
  return open({
    multiple: false,
    filters: [{ name: "GGUF Model", extensions: ["gguf"] }]
  }).then((selected) =>
    selected && typeof selected === "string" ? selected : null
  );
}

/* ------------------------------------------------------------------ */
/*  ModelCard                                                          */
/* ------------------------------------------------------------------ */

type ModelRoleKey = "chat" | "graph" | "embed";

const ROLE_META: Record<
  ModelRoleKey,
  { label: string; icon: React.ElementType; color: string }
> = {
  chat: { label: "对话模型", icon: MessageSquare, color: "text-sky-400" },
  graph: { label: "图谱模型", icon: Share2, color: "text-violet-400" },
  embed: { label: "嵌入模型", icon: Network, color: "text-emerald-400" }
};

type ModelCardProps = {
  role: ModelRoleKey;
  endpoint: string;
  model: string;
  contextLength?: number | null;
  concurrency?: number | null;
  isLocal: boolean;
  busy: boolean;
  onEndpointChange: (v: string) => void;
  onModelChange: (v: string) => void;
  onContextLengthChange: (v: number | null) => void;
  onConcurrencyChange: (v: number | null) => void;
  onPickFile: () => void;
  onProbe: () => void;
};

function ModelCard({
  role,
  endpoint,
  model,
  contextLength,
  concurrency,
  isLocal,
  busy,
  onEndpointChange,
  onModelChange,
  onContextLengthChange,
  onConcurrencyChange,
  onPickFile,
  onProbe
}: ModelCardProps) {
  const [expanded, setExpanded] = useState(false);
  const meta = ROLE_META[role];
  const Icon = meta.icon;
  const port = extractPort(endpoint);

  return (
    <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] overflow-hidden">
      {/* Header — always visible */}
      <div className="flex items-center gap-3 px-4 py-3">
        <div className={`flex h-8 w-8 items-center justify-center rounded-lg bg-[var(--bg-surface-2)] ${meta.color}`}>
          <Icon className="h-4 w-4" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-[var(--text-primary)]">{meta.label}</span>
            {model && (
              <span className="truncate text-xs text-[var(--text-secondary)]">{model}</span>
            )}
          </div>
          {port && (
            <div className="text-[11px] text-[var(--text-muted)] font-mono">端口 {port}</div>
          )}
        </div>
        <div className="flex items-center gap-1.5">
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
            onClick={() => setExpanded((p) => !p)}
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-2)] hover:text-[var(--text-primary)]"
          >
            {expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
          </button>
        </div>
      </div>

      {/* Expanded detail */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="border-t border-[var(--border-subtle)] px-4 py-3 space-y-3">
              {/* Model name */}
              <div className="space-y-1">
                <label className="text-[11px] font-medium text-[var(--text-muted)]">模型名称</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={model}
                    onChange={(e) => onModelChange(e.target.value)}
                    className="flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                    placeholder={role === "chat" ? "qwen3-14b" : role === "graph" ? "qwen3-8b" : "Qwen3-Embedding-4B"}
                  />
                  {isLocal && (
                    <button
                      type="button"
                      onClick={onPickFile}
                      className="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
                    >
                      <FolderOpen className="h-3.5 w-3.5" />
                      浏览
                    </button>
                  )}
                </div>
              </div>

              {/* Endpoint port */}
              <div className="space-y-1">
                <label className="text-[11px] font-medium text-[var(--text-muted)]">端口号</label>
                <input
                  type="text"
                  value={port}
                  onChange={(e) => onEndpointChange(replacePort(endpoint, e.target.value))}
                  className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)] font-mono"
                  placeholder="18001"
                />
              </div>

              {/* Context length & Concurrency */}
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
                    placeholder="32768"
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
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  ModelsTab                                                          */
/* ------------------------------------------------------------------ */

type ModelsTabProps = {
  t: TranslateFn;
  modelSettings: ModelSettingsDto;
  modelAvailability: ModelAvailabilityDto | null;
  providerModels: ProviderModelsDto;
  modelBusy: boolean;
  onProviderSwitch: (provider: ModelProvider) => void;
  onModelSettingsChange: (next: ModelSettingsDto) => void;
  onSaveModelSettings: () => Promise<void>;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
  onPickLocalModelsRoot: () => Promise<void>;
  onClearLocalModelsRoot: () => void;
};

export function ModelsTab({
  t,
  modelSettings,
  modelAvailability,
  providerModels,
  modelBusy,
  onProviderSwitch,
  onModelSettingsChange,
  onSaveModelSettings,
  onProbeModelProvider,
  onRefreshProviderModels,
  onPickLocalModelsRoot,
  onClearLocalModelsRoot
}: ModelsTabProps) {
  const activeProvider = modelSettings.active_provider;
  const isLocal = activeProvider === "ollama_local";
  const profile = isLocal ? modelSettings.local_profile : modelSettings.remote_profile;

  const updateProfile = (patch: Partial<LocalModelProfileDto & RemoteModelProfileDto>) => {
    if (isLocal) {
      onModelSettingsChange({
        ...modelSettings,
        local_profile: { ...modelSettings.local_profile, ...patch }
      });
    } else {
      onModelSettingsChange({
        ...modelSettings,
        remote_profile: { ...modelSettings.remote_profile, ...patch }
      });
    }
  };

  const handlePickFile = async (role: ModelRoleKey) => {
    const path = await pickModelFile();
    if (!path) return;
    const fileName = path.split(/[/\\]/).pop() ?? path;
    const dirPath = path.substring(0, path.lastIndexOf(fileName) - 1);
    const patch: Partial<LocalModelProfileDto> = {};
    if (role === "chat") patch.chat_model = fileName;
    else if (role === "graph") patch.graph_model = fileName;
    else patch.embed_model = fileName;
    // Auto-update models_root if the selected file is outside current root
    const currentRoot = modelSettings.local_profile.models_root;
    if (dirPath && (!currentRoot || !path.startsWith(currentRoot))) {
      patch.models_root = dirPath;
    }
    if (Object.keys(patch).length > 0) {
      updateProfile(patch);
    }
  };

  const statusOk = modelAvailability?.reachable && modelAvailability?.missing_roles?.length === 0;

  return (
    <motion.div
      key="settings-tab-models"
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      transition={{ duration: 0.22, ease: "easeOut" }}
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">
        {t("models")}
      </h3>

      <div className="space-y-4 pb-2">
        {/* Provider switch */}
        <div className="flex items-center justify-between rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
          <div className="flex items-center gap-2">
            <Bot className="h-4 w-4 text-[var(--accent)]" />
            <span className="text-sm font-medium text-[var(--text-primary)]">模型提供商</span>
          </div>
          <div className="flex rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] p-0.5">
            <button
              type="button"
              onClick={() => onProviderSwitch("ollama_local")}
              className={`rounded-md px-3 py-1 text-xs font-medium transition ${
                isLocal
                  ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              }`}
            >
              本地 (llama.cpp)
            </button>
            <button
              type="button"
              onClick={() => onProviderSwitch("openai_compatible")}
              className={`rounded-md px-3 py-1 text-xs font-medium transition ${
                !isLocal
                  ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              }`}
            >
              远程 (OpenAI)
            </button>
          </div>
        </div>

        {/* Local models root */}
        {isLocal && (
          <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3 space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-medium text-[var(--text-secondary)]">模型根目录</span>
              <div className="flex gap-1.5">
                <button
                  type="button"
                  onClick={() => void onPickLocalModelsRoot()}
                  className="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1 text-[11px] text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
                >
                  <FolderOpen className="h-3 w-3" />
                  选择目录
                </button>
                {modelSettings.local_profile.models_root && (
                  <button
                    type="button"
                    onClick={onClearLocalModelsRoot}
                    className="rounded-md px-2 py-1 text-[11px] text-[var(--text-muted)] transition hover:text-red-400"
                  >
                    清除
                  </button>
                )}
              </div>
            </div>
            {modelSettings.local_profile.models_root ? (
              <div className="font-mono text-[11px] text-[var(--text-muted)] truncate">
                {modelSettings.local_profile.models_root}
              </div>
            ) : (
              <div className="text-[11px] text-[var(--text-muted)]">未设置</div>
            )}
            {/* Model candidates summary */}
            <div className="flex gap-3 pt-1">
              <div className="text-[11px] text-[var(--text-muted)]">
                本地文件 <span className="text-[var(--text-primary)] font-medium">{providerModels.from_folder.length}</span>
              </div>
              <div className="text-[11px] text-[var(--text-muted)]">
                服务发现 <span className="text-[var(--text-primary)] font-medium">{providerModels.from_service.length}</span>
              </div>
              <button
                type="button"
                onClick={() => void onRefreshProviderModels()}
                disabled={modelBusy}
                className="ml-auto inline-flex items-center gap-1 text-[11px] text-[var(--accent)] transition hover:opacity-80 disabled:opacity-50"
              >
                <RefreshCw className={`h-3 w-3 ${modelBusy ? "animate-spin" : ""}`} />
                刷新
              </button>
            </div>
          </div>
        )}

        {/* Remote API Key */}
        {!isLocal && (
          <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3 space-y-2">
            <label className="text-xs font-medium text-[var(--text-secondary)]">API Key</label>
            <input
              type="password"
              value={modelSettings.remote_profile.api_key ?? ""}
              onChange={(e) =>
                onModelSettingsChange({
                  ...modelSettings,
                  remote_profile: { ...modelSettings.remote_profile, api_key: e.target.value }
                })
              }
              className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
              placeholder="sk-..."
            />
          </div>
        )}

        {/* Three model cards */}
        <ModelCard
          role="chat"
          endpoint={profile.chat_endpoint}
          model={profile.chat_model}
          contextLength={profile.chat_context_length}
          concurrency={profile.chat_concurrency}
          isLocal={isLocal}
          busy={modelBusy}
          onEndpointChange={(v) => updateProfile({ chat_endpoint: v })}
          onModelChange={(v) => updateProfile({ chat_model: v })}
          onContextLengthChange={(v) => updateProfile({ chat_context_length: v })}
          onConcurrencyChange={(v) => updateProfile({ chat_concurrency: v })}
          onPickFile={() => void handlePickFile("chat")}
          onProbe={() => void onProbeModelProvider()}
        />
        <ModelCard
          role="graph"
          endpoint={profile.graph_endpoint}
          model={profile.graph_model}
          contextLength={profile.graph_context_length}
          concurrency={profile.graph_concurrency}
          isLocal={isLocal}
          busy={modelBusy}
          onEndpointChange={(v) => updateProfile({ graph_endpoint: v })}
          onModelChange={(v) => updateProfile({ graph_model: v })}
          onContextLengthChange={(v) => updateProfile({ graph_context_length: v })}
          onConcurrencyChange={(v) => updateProfile({ graph_concurrency: v })}
          onPickFile={() => void handlePickFile("graph")}
          onProbe={() => void onProbeModelProvider()}
        />
        <ModelCard
          role="embed"
          endpoint={profile.embed_endpoint}
          model={profile.embed_model}
          contextLength={profile.embed_context_length}
          concurrency={profile.embed_concurrency}
          isLocal={isLocal}
          busy={modelBusy}
          onEndpointChange={(v) => updateProfile({ embed_endpoint: v })}
          onModelChange={(v) => updateProfile({ embed_model: v })}
          onContextLengthChange={(v) => updateProfile({ embed_context_length: v })}
          onConcurrencyChange={(v) => updateProfile({ embed_concurrency: v })}
          onPickFile={() => void handlePickFile("embed")}
          onProbe={() => void onProbeModelProvider()}
        />

        {/* Status bar */}
        {modelAvailability && (
          <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
            <div className="flex items-center gap-2 text-xs">
              <span className={`h-2 w-2 rounded-full ${statusOk ? "bg-emerald-400" : modelAvailability.reachable ? "bg-amber-400" : "bg-red-400"}`} />
              <span className="text-[var(--text-secondary)]">
                {statusOk
                  ? "所有模型就绪"
                  : modelAvailability.reachable
                    ? `部分就绪（缺少 ${modelAvailability.missing_roles.join(", ")}）`
                    : "连接失败"}
              </span>
            </div>
            {modelAvailability.errors.length > 0 && (
              <div className="mt-2 space-y-1">
                {modelAvailability.errors.map((err, i) => (
                  <div key={i} className="text-[11px] text-red-400">
                    {err.code}: {err.message}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Save button */}
        <div className="flex justify-end pt-2">
          <AnimatedPressButton
            type="button"
            onClick={() => void onSaveModelSettings()}
            disabled={modelBusy}
            className="inline-flex items-center gap-2 rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-white shadow-sm transition hover:opacity-90 disabled:opacity-50"
          >
            {modelBusy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
            保存配置
          </AnimatedPressButton>
        </div>
      </div>
    </motion.div>
  );
}
