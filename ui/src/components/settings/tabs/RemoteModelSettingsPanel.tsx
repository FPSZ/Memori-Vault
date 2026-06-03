import { RefreshCw } from "lucide-react";
import { CyberToggle } from "../../UI";
import type { ModelSettingsDto, RemoteModelProfileDto } from "../types";
import {
  REMOTE_PROTOCOLS,
  apiUrlToEndpoint,
  endpointToApiUrl,
  type RemoteProtocol,
  type RemoteProviderPreset,
} from "./modelUtils";

type RemoteModelSettingsPanelProps = {
  modelSettings: ModelSettingsDto;
  modelBusy: boolean;
  remoteProtocol: RemoteProtocol;
  remoteFullUrl: boolean;
  savedRemotePresets: RemoteProviderPreset[];
  customPresetName: string;
  remoteModels: string[];
  onRefreshProviderModels: () => Promise<void>;
  onRemoteProfileChange: (patch: Partial<RemoteModelProfileDto>) => void;
  onRemoteProtocolChange: (next: RemoteProtocol) => void;
  onRemoteFullUrlChange: (next: boolean) => void;
  onApplyRemotePreset: (preset: RemoteProviderPreset) => void;
  onDeleteRemotePreset: (id: string) => void;
  onCustomPresetNameChange: (next: string) => void;
  onSaveRemotePreset: () => void;
};

export function RemoteModelSettingsPanel({
  modelSettings,
  modelBusy,
  remoteProtocol,
  remoteFullUrl,
  savedRemotePresets,
  customPresetName,
  remoteModels,
  onRefreshProviderModels,
  onRemoteProfileChange,
  onRemoteProtocolChange,
  onRemoteFullUrlChange,
  onApplyRemotePreset,
  onDeleteRemotePreset,
  onCustomPresetNameChange,
  onSaveRemotePreset,
}: RemoteModelSettingsPanelProps) {
  const remoteApiUrl = endpointToApiUrl(modelSettings.remote_profile.chat_endpoint, remoteFullUrl);
  const remoteConfigToml = [
    'model_provider = "openai_compatible"',
    `model = "${modelSettings.remote_profile.chat_model}"`,
    `graph_model = "${modelSettings.remote_profile.graph_model}"`,
    `embed_model = "${modelSettings.remote_profile.embed_model}"`,
    `base_url = "${modelSettings.remote_profile.chat_endpoint}"`,
    `protocol = "${remoteProtocol}"`,
    `full_url_input = ${remoteFullUrl ? "true" : "false"}`,
    'network_access = "enabled"'
  ].join("\n");
  const remoteAuthJson = JSON.stringify(
    {
      OPENAI_API_KEY: modelSettings.remote_profile.api_key ? "********" : ""
    },
    null,
    2
  );

  const updateRemoteApiUrl = (value: string) => {
    const endpoint = apiUrlToEndpoint(value, remoteFullUrl);
    onRemoteProfileChange({
      chat_endpoint: endpoint,
      graph_endpoint: endpoint,
      embed_endpoint: endpoint
    });
  };

  return (
    <div className="space-y-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium text-[var(--text-primary)]">远程协议配置</div>
          <div className="mt-1 text-xs leading-relaxed text-[var(--text-muted)]">
            按协议、URL、Key 和模型名配置远端模型；供应商模板只用于快速填充。
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
        <div className="space-y-1">
          <label className="text-[11px] font-medium text-[var(--text-muted)]">协议</label>
          <select
            value={remoteProtocol}
            onChange={(e) => onRemoteProtocolChange(e.target.value as RemoteProtocol)}
            className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
          >
            {REMOTE_PROTOCOLS.map((item) => (
              <option key={item.value} value={item.value}>
                {item.label}
              </option>
            ))}
          </select>
          <div className="text-[11px] leading-relaxed text-[var(--text-muted)]">
            {REMOTE_PROTOCOLS.find((item) => item.value === remoteProtocol)?.description}
          </div>
        </div>
      </div>

      <div className="space-y-1">
        <div className="flex items-center justify-between gap-3">
          <label className="text-[11px] font-medium text-[var(--text-muted)]">API 请求地址</label>
          <div className="flex items-center gap-2 text-[11px] text-[var(--text-muted)]">
            <span>完整 URL</span>
            <CyberToggle
              checked={remoteFullUrl}
              onChange={onRemoteFullUrlChange}
              ariaLabel="完整 URL"
            />
          </div>
        </div>
        <input
          type="text"
          value={remoteApiUrl}
          onChange={(e) => updateRemoteApiUrl(e.target.value)}
          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 font-mono text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
          placeholder={remoteFullUrl ? "https://example.com/v1" : "https://example.com"}
        />
        <div className="rounded-lg border border-amber-400/30 bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-amber-500">
          {remoteFullUrl
            ? "已开启完整 URL：可以填写带 /v1 的兼容接口地址，系统会自动避免重复拼接 /v1。"
            : "填写兼容 OpenAI Response / Chat 格式的服务端点地址；系统会自动请求 /v1/models，并按协议使用 /v1/chat/completions 或 /v1/responses。"}
        </div>
      </div>

      <div className="space-y-1">
        <label className="text-[11px] font-medium text-[var(--text-muted)]">API Key</label>
        <input
          type="password"
          value={modelSettings.remote_profile.api_key ?? ""}
          onChange={(e) => onRemoteProfileChange({ api_key: e.target.value })}
          className="w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
          placeholder="sk-..."
        />
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
