import { useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  Bot,
  ChevronDown,
  ChevronUp,
  FolderOpen,
  LoaderCircle,
  MessageSquare,
  Network,
  Play,
  RefreshCw,
  Save,
  Share2,
  Square,
  Zap
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { AnimatedPressButton } from "../../MotionKit";
import type {
  LocalModelProfileDto,
  LocalModelRuntimeStatusDto,
  LocalModelRuntimeStatusesDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto,
  RemoteModelProfileDto
} from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];
type ModelRoleKey = "chat" | "graph" | "embed";

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

function pickLlamaServerFile(): Promise<string | null> {
  return open({
    multiple: false,
    filters: [{ name: "llama-server", extensions: ["exe", ""] }]
  }).then((selected) =>
    selected && typeof selected === "string" ? selected : null
  );
}

function fileNameFromPath(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

function dirNameFromPath(path: string): string {
  const name = fileNameFromPath(path);
  const index = path.lastIndexOf(name);
  return index > 0 ? path.slice(0, index).replace(/[\\/]$/, "") : "";
}

function modelPathForRole(profile: LocalModelProfileDto, role: ModelRoleKey): string {
  if (role === "chat") return profile.chat_model_path ?? "";
  if (role === "graph") return profile.graph_model_path ?? "";
  return profile.embed_model_path ?? "";
}

function runtimeStatusForRole(
  statuses: LocalModelRuntimeStatusesDto | null,
  role: ModelRoleKey
): LocalModelRuntimeStatusDto | null {
  return statuses?.roles.find((item) => item.role === role) ?? null;
}

const ROLE_META: Record<
  ModelRoleKey,
  { label: string; icon: React.ElementType; color: string; defaultModel: string; defaultPort: string }
> = {
  chat: {
    label: "对话模型",
    icon: MessageSquare,
    color: "text-sky-400",
    defaultModel: "qwen3-14b",
    defaultPort: "18001"
  },
  graph: {
    label: "图谱模型",
    icon: Share2,
    color: "text-violet-400",
    defaultModel: "qwen3-8b",
    defaultPort: "18002"
  },
  embed: {
    label: "向量模型",
    icon: Network,
    color: "text-emerald-400",
    defaultModel: "Qwen3-Embedding-4B",
    defaultPort: "18003"
  }
};

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
  onEndpointChange: (v: string) => void;
  onModelChange: (v: string) => void;
  onContextLengthChange: (v: number | null) => void;
  onConcurrencyChange: (v: number | null) => void;
  onPickFile: () => void;
  onProbe: () => void;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
};

function ModelCard({
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
  onEndpointChange,
  onModelChange,
  onContextLengthChange,
  onConcurrencyChange,
  onPickFile,
  onProbe,
  onStart,
  onStop,
  onRestart
}: ModelCardProps) {
  const [expanded, setExpanded] = useState(false);
  const meta = ROLE_META[role];
  const Icon = meta.icon;
  const port = extractPort(endpoint);
  const running = runtimeStatus?.state === "running";
  const stateLabel =
    runtimeStatus?.state === "running"
      ? "运行中"
      : runtimeStatus?.state === "error"
        ? "异常"
        : "未启动";

  return (
    <div className="overflow-hidden rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)]">
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
              <span className={running ? "text-emerald-400" : runtimeStatus?.state === "error" ? "text-red-400" : ""}>
                {stateLabel}{runtimeStatus?.pid ? ` · PID ${runtimeStatus.pid}` : ""}
              </span>
            ) : null}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          {isLocal ? (
            running ? (
              <button
                type="button"
                onClick={onStop}
                disabled={runtimeBusy}
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
            onClick={() => setExpanded((p) => !p)}
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

              {isLocal && runtimeStatus?.message ? (
                <div className="rounded-lg bg-[var(--bg-surface-2)] px-3 py-2 text-[11px] text-[var(--text-muted)]">
                  {runtimeStatus.message}
                </div>
              ) : null}

              {isLocal && running ? (
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

type ModelsTabProps = {
  t: TranslateFn;
  modelSettings: ModelSettingsDto;
  modelAvailability: ModelAvailabilityDto | null;
  providerModels: ProviderModelsDto;
  modelBusy: boolean;
  localModelRuntimeStatuses: LocalModelRuntimeStatusesDto | null;
  localModelRuntimeBusyRole: string | null;
  onProviderSwitch: (provider: ModelProvider) => void;
  onModelSettingsChange: (next: ModelSettingsDto) => void;
  onSaveModelSettings: () => Promise<void>;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
  onRefreshLocalModelRuntimeStatus: () => Promise<void>;
  onStartLocalModel: (role: ModelRoleKey) => Promise<void>;
  onStopLocalModel: (role: ModelRoleKey) => Promise<void>;
  onRestartLocalModel: (role: ModelRoleKey) => Promise<void>;
  onPickLocalModelsRoot: () => Promise<void>;
  onClearLocalModelsRoot: () => void;
};

export function ModelsTab({
  t,
  modelSettings,
  modelAvailability,
  providerModels,
  modelBusy,
  localModelRuntimeStatuses,
  localModelRuntimeBusyRole,
  onProviderSwitch,
  onModelSettingsChange,
  onSaveModelSettings,
  onProbeModelProvider,
  onRefreshProviderModels,
  onRefreshLocalModelRuntimeStatus,
  onStartLocalModel,
  onStopLocalModel,
  onRestartLocalModel,
  onPickLocalModelsRoot,
  onClearLocalModelsRoot
}: ModelsTabProps) {
  const activeProvider = modelSettings.active_provider;
  const isLocal = activeProvider === "llama_cpp_local";
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

  const handlePickLlamaServer = async () => {
    const path = await pickLlamaServerFile();
    if (path) {
      updateProfile({ llama_server_path: path });
    }
  };

  const handlePickFile = async (role: ModelRoleKey) => {
    const path = await pickModelFile();
    if (!path) return;
    const fileName = fileNameFromPath(path);
    const stem = fileName.replace(/\.gguf$/i, "");
    const dirPath = dirNameFromPath(path);
    const patch: Partial<LocalModelProfileDto> = {};
    if (role === "chat") {
      patch.chat_model = stem;
      patch.chat_model_path = path;
    } else if (role === "graph") {
      patch.graph_model = stem;
      patch.graph_model_path = path;
    } else {
      patch.embed_model = stem;
      patch.embed_model_path = path;
    }
    const currentRoot = modelSettings.local_profile.models_root;
    if (dirPath && (!currentRoot || !path.startsWith(currentRoot))) {
      patch.models_root = dirPath;
    }
    updateProfile(patch);
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
        <div className="flex items-center justify-between rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
          <div className="flex items-center gap-2">
            <Bot className="h-4 w-4 text-[var(--accent)]" />
            <span className="text-sm font-medium text-[var(--text-primary)]">模型提供商</span>
          </div>
          <div className="flex rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] p-0.5">
            <button
              type="button"
              onClick={() => onProviderSwitch("llama_cpp_local")}
              className={`rounded-md px-3 py-1 text-xs font-medium transition ${
                isLocal
                  ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                  : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              }`}
            >
              本地 llama.cpp
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
              远程 OpenAI-compatible
            </button>
          </div>
        </div>

        {isLocal ? (
          <div className="space-y-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-[var(--text-secondary)]">llama-server 可执行文件</span>
                <div className="flex gap-1.5">
                  <button
                    type="button"
                    onClick={() => void handlePickLlamaServer()}
                    className="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1 text-[11px] text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
                  >
                    <FolderOpen className="h-3 w-3" />
                    选择文件
                  </button>
                  {modelSettings.local_profile.llama_server_path ? (
                    <button
                      type="button"
                      onClick={() => updateProfile({ llama_server_path: "" })}
                      className="rounded-md px-2 py-1 text-[11px] text-[var(--text-muted)] transition hover:text-red-400"
                    >
                      清除
                    </button>
                  ) : null}
                </div>
              </div>
              <div className="truncate font-mono text-[11px] text-[var(--text-muted)]">
                {modelSettings.local_profile.llama_server_path || "未设置时会尝试从 PATH 查找 llama-server"}
              </div>
            </div>

            <div className="space-y-1">
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
                  {modelSettings.local_profile.models_root ? (
                    <button
                      type="button"
                      onClick={onClearLocalModelsRoot}
                      className="rounded-md px-2 py-1 text-[11px] text-[var(--text-muted)] transition hover:text-red-400"
                    >
                      清除
                    </button>
                  ) : null}
                </div>
              </div>
              <div className="truncate font-mono text-[11px] text-[var(--text-muted)]">
                {modelSettings.local_profile.models_root || "未设置"}
              </div>
            </div>

            <div className="flex gap-3 pt-1">
              <div className="text-[11px] text-[var(--text-muted)]">
                本地文件 <span className="font-medium text-[var(--text-primary)]">{providerModels.from_folder.length}</span>
              </div>
              <div className="text-[11px] text-[var(--text-muted)]">
                服务发现 <span className="font-medium text-[var(--text-primary)]">{providerModels.from_service.length}</span>
              </div>
              <button
                type="button"
                onClick={() => void onRefreshProviderModels()}
                disabled={modelBusy}
                className="ml-auto inline-flex items-center gap-1 text-[11px] text-[var(--accent)] transition hover:opacity-80 disabled:opacity-50"
              >
                <RefreshCw className={`h-3 w-3 ${modelBusy ? "animate-spin" : ""}`} />
                刷新模型
              </button>
              <button
                type="button"
                onClick={() => void onRefreshLocalModelRuntimeStatus()}
                disabled={Boolean(localModelRuntimeBusyRole)}
                className="inline-flex items-center gap-1 text-[11px] text-[var(--accent)] transition hover:opacity-80 disabled:opacity-50"
              >
                <RefreshCw className={`h-3 w-3 ${localModelRuntimeBusyRole ? "animate-spin" : ""}`} />
                刷新运行状态
              </button>
            </div>
          </div>
        ) : null}

        {!isLocal ? (
          <div className="space-y-2 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
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
        ) : null}

        {(["chat", "graph", "embed"] as const).map((role) => (
          <ModelCard
            key={role}
            role={role}
            endpoint={profile[`${role}_endpoint` as keyof typeof profile] as string}
            model={profile[`${role}_model` as keyof typeof profile] as string}
            modelPath={isLocal ? modelPathForRole(modelSettings.local_profile, role) : ""}
            contextLength={profile[`${role}_context_length` as keyof typeof profile] as number | null | undefined}
            concurrency={profile[`${role}_concurrency` as keyof typeof profile] as number | null | undefined}
            runtimeStatus={runtimeStatusForRole(localModelRuntimeStatuses, role)}
            isLocal={isLocal}
            busy={modelBusy}
            runtimeBusy={localModelRuntimeBusyRole === role}
            onEndpointChange={(v) => updateProfile({ [`${role}_endpoint`]: v })}
            onModelChange={(v) => updateProfile({ [`${role}_model`]: v })}
            onContextLengthChange={(v) => updateProfile({ [`${role}_context_length`]: v })}
            onConcurrencyChange={(v) => updateProfile({ [`${role}_concurrency`]: v })}
            onPickFile={() => void handlePickFile(role)}
            onProbe={() => void onProbeModelProvider()}
            onStart={() => void onStartLocalModel(role)}
            onStop={() => void onStopLocalModel(role)}
            onRestart={() => void onRestartLocalModel(role)}
          />
        ))}

        {modelAvailability ? (
          <div className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
            <div className="flex items-center gap-2 text-xs">
              <span className={`h-2 w-2 rounded-full ${statusOk ? "bg-emerald-400" : modelAvailability.reachable ? "bg-amber-400" : "bg-red-400"}`} />
              <span className="text-[var(--text-secondary)]">
                {statusOk
                  ? "所有模型已就绪"
                  : modelAvailability.reachable
                    ? `部分就绪，缺少 ${modelAvailability.missing_roles.join(", ")}`
                    : "连接失败"}
              </span>
            </div>
            {modelAvailability.errors.length > 0 ? (
              <div className="mt-2 space-y-1">
                {modelAvailability.errors.map((err, i) => (
                  <div key={i} className="text-[11px] text-red-400">
                    {err.code}: {err.message}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}

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
