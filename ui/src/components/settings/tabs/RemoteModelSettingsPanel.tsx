import { useState } from "react";
import { Eye, EyeOff, LoaderCircle, RefreshCw, RotateCcw } from "lucide-react";
import { testRemoteConnection } from "../../../app/api/desktop";
import type { ModelSettingsDto, RemoteModelProfileDto } from "../types";
import {
  REMOTE_API_FORMATS,
  buildOpenAiUrl,
  normalizeRemoteBaseUrl,
  type RemoteApiFormat,
  type RemoteProviderPreset,
} from "./modelUtils";

type RemoteModelSettingsPanelProps = {
  modelSettings: ModelSettingsDto;
  modelBusy: boolean;
  remoteApiFormat: RemoteApiFormat;
  savedRemotePresets: RemoteProviderPreset[];
  customPresetName: string;
  remoteModels: string[];
  onRefreshProviderModels: () => Promise<void>;
  onRemoteProfileChange: (patch: Partial<RemoteModelProfileDto>) => void;
  onRemoteApiFormatChange: (next: RemoteApiFormat) => void;
  onApplyRemotePreset: (preset: RemoteProviderPreset) => void;
  onDeleteRemotePreset: (id: string) => void;
  onCustomPresetNameChange: (next: string) => void;
  onSaveRemotePreset: () => void;
};

export function RemoteModelSettingsPanel({
  modelSettings,
  modelBusy,
  remoteApiFormat,
  savedRemotePresets,
  customPresetName,
  remoteModels,
  onRefreshProviderModels,
  onRemoteProfileChange,
  onRemoteApiFormatChange,
  onApplyRemotePreset,
  onDeleteRemotePreset,
  onCustomPresetNameChange,
  onSaveRemotePreset,
}: RemoteModelSettingsPanelProps) {
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);
  const baseUrl = modelSettings.remote_profile.chat_endpoint;
  const selectedFormat = REMOTE_API_FORMATS.find((item) => item.value === remoteApiFormat) ?? REMOTE_API_FORMATS[0];
  const previewUrl = baseUrl.trim() ? buildOpenAiUrl(baseUrl, selectedFormat.tail) : "";
  const remoteConfigToml = [
    'model_provider = "openai_compatible"',
    `model = "${modelSettings.remote_profile.chat_model}"`,
    `graph_model = "${modelSettings.remote_profile.graph_model}"`,
    `embed_model = "${modelSettings.remote_profile.embed_model}"`,
    `base_url = "${modelSettings.remote_profile.chat_endpoint}"`,
    `api_format = "${remoteApiFormat}"`,
    'network_access = "enabled"'
  ].join("\n");
  const remoteAuthJson = JSON.stringify(
    {
      OPENAI_API_KEY: modelSettings.remote_profile.api_key ? "********" : ""
    },
    null,
    2
  );

  const updateRemoteBaseUrl = (value: string) => {
    const endpoint = normalizeRemoteBaseUrl(value);
    onRemoteProfileChange({
      chat_endpoint: endpoint,
      graph_endpoint: endpoint,
      embed_endpoint: endpoint
    });
  };

  const runConnectionTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const message = await testRemoteConnection({
        baseUrl,
        apiKey: modelSettings.remote_profile.api_key ?? null,
        apiFormat: remoteApiFormat,
        chatModel: modelSettings.remote_profile.chat_model
      });
      setTestResult({ ok: true, message });
    } catch (error) {
      setTestResult({ ok: false, message: error instanceof Error ? error.message : String(error) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="space-y-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium text-[var(--text-primary)]">远程提供商配置</div>
          <div className="mt-1 text-xs leading-relaxed text-[var(--text-muted)]">
            按 Cherry Studio 的地址规则配置：末尾 # 强制完整 URL，末尾 / 不补 /v1，其它地址自动补 /v1。
          </div>
        </div>
        <button
          type="button"
          onClick={() => void onRefreshProviderModels()}
          disabled={modelBusy}
          className="inline-flex shrink-0 items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1 text-[11px] text-[var(--accent)] transition hover:bg-[var(--bg-surface-1)] disabled:opacity-50"
        >
          <RefreshCw className={`h-3 w-3 ${modelBusy ? "animate-spin" : ""}`} />
          获取模型列表
        </button>
      </div>

      <div className="space-y-1">
        <label className="text-[11px] font-medium text-[var(--text-muted)]">API 协议</label>
        <select
          value={remoteApiFormat}
          onChange={(e) => onRemoteApiFormatChange(e.target.value as RemoteApiFormat)}
          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
        >
          {REMOTE_API_FORMATS.map((item) => (
            <option key={item.value} value={item.value}>
              {item.label}
            </option>
          ))}
        </select>
        <div className="text-[11px] leading-relaxed text-[var(--text-muted)]">
          {selectedFormat.description}
        </div>
      </div>

      <div className="space-y-1">
        <div className="flex items-center justify-between gap-3">
          <label className="text-[11px] font-medium text-[var(--text-muted)]">API 密钥</label>
          <button
            type="button"
            onClick={() => void runConnectionTest()}
            disabled={testing || !baseUrl.trim() || !modelSettings.remote_profile.chat_model.trim()}
            className="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1 text-[11px] text-[var(--accent)] transition hover:bg-[var(--bg-surface-1)] disabled:cursor-not-allowed disabled:opacity-50"
          >
            {testing ? <LoaderCircle className="h-3 w-3 animate-spin" /> : null}
            检测
          </button>
        </div>
        <div className="flex overflow-hidden rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] focus-within:border-[var(--accent)]">
          <input
            type={apiKeyVisible ? "text" : "password"}
            value={modelSettings.remote_profile.api_key ?? ""}
            onChange={(e) => onRemoteProfileChange({ api_key: e.target.value })}
            className="min-w-0 flex-1 bg-transparent px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none"
            placeholder="sk-..."
          />
          <button
            type="button"
            onClick={() => setApiKeyVisible((prev) => !prev)}
            className="inline-flex w-10 items-center justify-center border-l border-[var(--border-subtle)] text-[var(--text-muted)] transition hover:text-[var(--text-primary)]"
            aria-label={apiKeyVisible ? "隐藏 API Key" : "显示 API Key"}
          >
            {apiKeyVisible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </button>
        </div>
        {testResult ? (
          <div className={`rounded-lg px-3 py-2 text-[11px] leading-relaxed ${testResult.ok ? "bg-emerald-500/10 text-emerald-500" : "bg-red-500/10 text-red-400"}`}>
            {testResult.message}
          </div>
        ) : null}
      </div>

      <div className="space-y-1">
        <div className="flex items-center justify-between gap-3">
          <label className="text-[11px] font-medium text-[var(--text-muted)]">API 地址</label>
          <button
            type="button"
            onClick={() => updateRemoteBaseUrl("")}
            className="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2 py-1 text-[11px] text-red-400 transition hover:bg-[var(--bg-surface-1)]"
          >
            <RotateCcw className="h-3 w-3" />
            重置
          </button>
        </div>
        <input
          type="text"
          value={baseUrl}
          onChange={(e) => updateRemoteBaseUrl(e.target.value)}
          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 font-mono text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
          placeholder="https://api.deepseek.com"
        />
        <div className="text-[11px] leading-relaxed text-[var(--text-muted)]">
          预览：{previewUrl || "填写 API 地址后显示最终请求 URL"}
        </div>
      </div>

      {savedRemotePresets.length > 0 ? (
        <div className="grid gap-2 sm:grid-cols-2">
          {savedRemotePresets.map((preset) => {
            const isCustom = preset.id.startsWith("custom-");
            return (
              <div
                key={preset.id}
                className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2"
              >
                <div className="flex items-start justify-between gap-2">
                  <button
                    type="button"
                    onClick={() => onApplyRemotePreset(preset)}
                    className="min-w-0 flex-1 text-left"
                  >
                    <div className="truncate text-xs font-medium text-[var(--text-primary)]">{preset.label}</div>
                    <div className="mt-1 line-clamp-2 text-[11px] leading-relaxed text-[var(--text-muted)]">
                      {preset.description}
                    </div>
                  </button>
                  {isCustom ? (
                    <button
                      type="button"
                      onClick={() => onDeleteRemotePreset(preset.id)}
                      className="rounded-md px-1.5 py-0.5 text-[11px] text-[var(--text-muted)] transition hover:text-red-400"
                    >
                      删除
                    </button>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      <div className="flex gap-2">
        <input
          type="text"
          value={customPresetName}
          onChange={(e) => onCustomPresetNameChange(e.target.value)}
          className="min-w-0 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
          placeholder="预设名称"
        />
        <button
          type="button"
          onClick={onSaveRemotePreset}
          disabled={!customPresetName.trim()}
          className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs font-medium text-[var(--accent)] transition hover:bg-[var(--bg-surface-1)] disabled:cursor-not-allowed disabled:opacity-50"
        >
          保存当前配置
        </button>
      </div>

      {remoteModels.length > 0 ? (
        <div className="rounded-lg border border-emerald-400/25 bg-emerald-500/10 px-3 py-2 text-[11px] leading-relaxed text-emerald-500">
          已获取 {remoteModels.length} 个远端模型。展开下面任意模型卡片，可以从“模型名称”输入框的候选列表中选择，也可以继续手动填写。
        </div>
      ) : null}

      <details className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2">
        <summary className="cursor-pointer text-xs font-medium text-[var(--text-secondary)]">
          可视化配置文件预览
        </summary>
        <div className="mt-3 space-y-3">
          <div>
            <div className="mb-1 text-[11px] font-medium text-[var(--text-muted)]">auth.json（已脱敏）</div>
            <pre className="overflow-x-auto rounded-lg bg-[var(--bg-surface-1)] px-3 py-2 font-mono text-[11px] text-[var(--text-secondary)]">
              {remoteAuthJson}
            </pre>
          </div>
          <div>
            <div className="mb-1 text-[11px] font-medium text-[var(--text-muted)]">config.toml</div>
            <pre className="overflow-x-auto rounded-lg bg-[var(--bg-surface-1)] px-3 py-2 font-mono text-[11px] text-[var(--text-secondary)]">
              {remoteConfigToml}
            </pre>
          </div>
        </div>
      </details>
    </div>
  );
}
