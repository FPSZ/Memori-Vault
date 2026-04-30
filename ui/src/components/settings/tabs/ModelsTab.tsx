import { useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  Bot,
  ChevronDown,
  ChevronUp,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import { CyberToggle } from "../../UI";
import type {
  LocalModelProfileDto,
  LocalModelRuntimeStatusesDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto,
  RemoteModelProfileDto
} from "../types";
import { useI18n } from "../../../i18n";
import {
  type TranslateFn,
  type ModelRoleKey,
  type RoleErrorMap,
  PERFORMANCE_PRESETS,
  pickModelFile,
  pickLlamaServerFile,
  fileNameFromPath,
  dirNameFromPath,
  modelPathForRole,
  runtimeStatusForRole,
  roleEndpoint,
  roleModel,
  optionalNumber,
  validateLocalRoles,
  describeAvailabilityError,
  ROLE_META,
} from "./modelUtils";
import { ModelCard } from "./ModelCard";

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
  const [expandedRoles, setExpandedRoles] = useState<Record<ModelRoleKey, boolean>>({
    chat: false,
    graph: false,
    embed: false
  });
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [localRoleErrors, setLocalRoleErrors] = useState<RoleErrorMap>({});
  const [localConfigMessages, setLocalConfigMessages] = useState<string[]>([]);

  const setRoleExpanded = (role: ModelRoleKey, expanded: boolean) => {
    setExpandedRoles((prev) => ({ ...prev, [role]: expanded }));
  };

  const expandRoles = (roles: readonly ModelRoleKey[]) => {
    setExpandedRoles((prev) => {
      const next = { ...prev };
      for (const role of roles) {
        next[role] = true;
      }
      return next;
    });
  };

  const showLocalValidation = (
    check: ReturnType<typeof validateLocalRoles>,
    fallbackMessage: string
  ) => {
    setLocalRoleErrors(check.roleErrors);
    const messages = [
      ...Object.values(check.roleErrors),
      ...check.generalErrors
    ].filter((message): message is string => Boolean(message));
    setLocalConfigMessages(messages.length > 0 ? messages : [fallbackMessage]);
    const rolesWithError = (["chat", "graph", "embed"] as const).filter((role) => check.roleErrors[role]);
    if (rolesWithError.length > 0) {
      expandRoles(rolesWithError);
    } else if (check.firstRole) {
      setRoleExpanded(check.firstRole, true);
    }
  };

  const clearLocalValidationForRole = (role: ModelRoleKey) => {
    setLocalRoleErrors((prev) => {
      if (!prev[role]) return prev;
      const next = { ...prev };
      delete next[role];
      return next;
    });
  };

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
      setLocalConfigMessages((prev) =>
        prev.filter((message) => !message.includes("llama-server"))
      );
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
    clearLocalValidationForRole(role);
    setRoleExpanded(role, true);
  };

  const statusOk = modelAvailability?.reachable && modelAvailability?.missing_roles?.length === 0;

  const handleStartLocalModel = async (role: ModelRoleKey) => {
    if (isLocal) {
      const check = validateLocalRoles(modelSettings.local_profile, [role]);
      if (!check.ok) {
        showLocalValidation(check, `${ROLE_META[role].label}配置不完整，无法启动。`);
        setRoleExpanded(role, true);
        return;
      }
      setLocalRoleErrors((prev) => {
        const next = { ...prev };
        delete next[role];
        return next;
      });
    }
    try {
      await onStartLocalModel(role);
      setLocalConfigMessages([]);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLocalRoleErrors((prev) => ({ ...prev, [role]: message }));
      setLocalConfigMessages([`${ROLE_META[role].label}启动失败：${message}`]);
      setRoleExpanded(role, true);
    }
  };

  const handleProbe = async () => {
    if (isLocal) {
      const check = validateLocalRoles(modelSettings.local_profile, ["chat", "graph", "embed"]);
      if (!check.ok) {
        showLocalValidation(check, "本地模型配置不完整，无法探测。");
        return;
      }
      setLocalRoleErrors({});
      setLocalConfigMessages(check.generalErrors);
    }
    try {
      await onProbeModelProvider();
    } catch {
      if (isLocal) {
        setLocalConfigMessages((prev) =>
          prev.length > 0
            ? prev
            : ["探测失败：配置已完整，但模型服务未连通。请确认三个模型已经启动，或检查端口是否被占用。"]
        );
      }
    }
  };

  const handleRefreshProviderModels = async () => {
    if (isLocal) {
      const check = validateLocalRoles(modelSettings.local_profile, ["chat", "graph", "embed"]);
      if (!check.ok) {
        showLocalValidation(check, "本地模型配置不完整，无法刷新模型列表。");
        return;
      }
      setLocalRoleErrors({});
      setLocalConfigMessages(check.generalErrors);
    }
    try {
      await onRefreshProviderModels();
    } catch {
      if (isLocal) {
        setLocalConfigMessages((prev) =>
          prev.length > 0
            ? prev
            : ["刷新模型列表失败：请确认模型服务已经启动，或检查端口配置。"]
        );
      }
    }
  };

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
          <div className="flex items-start justify-between gap-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
            <div className="min-w-0">
              <div className="text-sm font-medium text-[var(--text-primary)]">退出时关闭本地模型</div>
              <div className="mt-1 text-xs leading-relaxed text-[var(--text-muted)]">
                关闭后，退出软件会保留已启动的 llama.cpp 进程；需要释放显存时在模型卡片里手动停止。
              </div>
            </div>
            <CyberToggle
              checked={modelSettings.stop_local_models_on_exit}
              onChange={(stopLocalModelsOnExit) =>
                onModelSettingsChange({
                  ...modelSettings,
                  stop_local_models_on_exit: stopLocalModelsOnExit
                })
              }
              ariaLabel="退出时关闭本地模型"
            />
          </div>
        ) : null}

        {isLocal ? (
          <div className="space-y-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
            {localConfigMessages.length > 0 ? (
              <div className="space-y-1 rounded-lg border border-amber-400/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-500">
                {localConfigMessages.map((message, index) => (
                  <div key={index}>{message}</div>
                ))}
              </div>
            ) : null}

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
                onClick={() => void handleRefreshProviderModels()}
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
            <div className="space-y-3 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <div className="text-xs font-medium text-[var(--text-secondary)]">llama.cpp 性能预设</div>
                  <div className="mt-0.5 text-[11px] text-[var(--text-muted)]">
                    默认用兼容模式；换电脑后如果启动失败，先切回兼容模式。
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => setAdvancedOpen((prev) => !prev)}
                  className="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] px-2 py-1 text-[11px] text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
                >
                  {advancedOpen ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />}
                  高级参数
                </button>
              </div>

              <div className="grid gap-2 sm:grid-cols-2">
                {PERFORMANCE_PRESETS.map((preset) => {
                  const selected = (modelSettings.local_profile.performance_preset ?? "compat") === preset.value;
                  return (
                    <button
                      key={preset.value}
                      type="button"
                      onClick={() => updateProfile({ performance_preset: preset.value })}
                      className={`rounded-lg border px-3 py-2 text-left transition ${
                        selected
                          ? "border-[var(--accent)] bg-[var(--accent-soft)]"
                          : "border-[var(--border-subtle)] bg-[var(--bg-surface-1)] hover:border-[var(--accent)]/50"
                      }`}
                    >
                      <div className="text-xs font-medium text-[var(--text-primary)]">{preset.label}</div>
                      <div className="mt-1 text-[11px] leading-relaxed text-[var(--text-muted)]">{preset.description}</div>
                    </button>
                  );
                })}
              </div>

              <AnimatePresence>
                {advancedOpen ? (
                  <motion.div
                    initial={{ height: 0, opacity: 0 }}
                    animate={{ height: "auto", opacity: 1 }}
                    exit={{ height: 0, opacity: 0 }}
                    transition={{ duration: 0.18 }}
                    className="overflow-hidden"
                  >
                    <div className="grid gap-3 border-t border-[var(--border-subtle)] pt-3 sm:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">GPU layers</label>
                        <input
                          type="number"
                          value={modelSettings.local_profile.n_gpu_layers ?? ""}
                          onChange={(e) => updateProfile({ n_gpu_layers: optionalNumber(e.target.value) })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="留空使用预设，-1 表示尽量全放 GPU"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">Batch size</label>
                        <input
                          type="number"
                          value={modelSettings.local_profile.batch_size ?? ""}
                          onChange={(e) => updateProfile({ batch_size: optionalNumber(e.target.value, 1) })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="例如 512 / 1024 / 2048"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">Ubatch size</label>
                        <input
                          type="number"
                          value={modelSettings.local_profile.ubatch_size ?? ""}
                          onChange={(e) => updateProfile({ ubatch_size: optionalNumber(e.target.value, 1) })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="显存不够就调小"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">CPU threads</label>
                        <input
                          type="number"
                          value={modelSettings.local_profile.threads ?? ""}
                          onChange={(e) => updateProfile({ threads: optionalNumber(e.target.value, 1) })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="留空自动"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">Batch threads</label>
                        <input
                          type="number"
                          value={modelSettings.local_profile.threads_batch ?? ""}
                          onChange={(e) => updateProfile({ threads_batch: optionalNumber(e.target.value, 1) })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="留空自动"
                        />
                      </div>
                      <label className="flex items-center gap-2 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                        <input
                          type="checkbox"
                          checked={Boolean(modelSettings.local_profile.flash_attn)}
                          onChange={(e) => updateProfile({ flash_attn: e.target.checked })}
                        />
                        开启 Flash Attention
                      </label>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">Cache type K</label>
                        <input
                          type="text"
                          value={modelSettings.local_profile.cache_type_k ?? ""}
                          onChange={(e) => updateProfile({ cache_type_k: e.target.value })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 font-mono text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="例如 f16 / q8_0"
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-[11px] font-medium text-[var(--text-muted)]">Cache type V</label>
                        <input
                          type="text"
                          value={modelSettings.local_profile.cache_type_v ?? ""}
                          onChange={(e) => updateProfile({ cache_type_v: e.target.value })}
                          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-1.5 font-mono text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                          placeholder="例如 f16 / q8_0"
                        />
                      </div>
                    </div>
                    <div className="mt-2 text-[11px] leading-relaxed text-[var(--text-muted)]">
                      高级参数会覆盖预设。参数改完后需要保存配置，并重启对应模型服务才会生效。
                    </div>
                  </motion.div>
                ) : null}
              </AnimatePresence>
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
            expanded={expandedRoles[role]}
            validationMessage={localRoleErrors[role] ?? null}
            onEndpointChange={(v) => updateProfile({ [`${role}_endpoint`]: v })}
            onModelChange={(v) => updateProfile({ [`${role}_model`]: v })}
            onContextLengthChange={(v) => updateProfile({ [`${role}_context_length`]: v })}
            onConcurrencyChange={(v) => updateProfile({ [`${role}_concurrency`]: v })}
            onPickFile={() => void handlePickFile(role)}
            onProbe={() => void handleProbe()}
            onStart={() => void handleStartLocalModel(role)}
            onStop={() => void onStopLocalModel(role)}
            onRestart={() => void onRestartLocalModel(role)}
            onExpandedChange={(expanded) => setRoleExpanded(role, expanded)}
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
                    {describeAvailabilityError(
                      err.code,
                      err.message,
                      isLocal ? modelSettings.local_profile : null
                    )}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}

      </div>
    </motion.div>
  );
}
