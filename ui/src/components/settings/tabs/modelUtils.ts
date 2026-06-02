import { MessageSquare, Network, Share2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  LocalPerformancePreset,
  LocalModelProfileDto,
  LocalModelRuntimeStatusDto,
  LocalModelRuntimeStatusesDto,
  RemoteModelProfileDto,
} from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];
type ModelRoleKey = "chat" | "graph" | "embed";
type RemoteProviderPreset = {
  id: string;
  label: string;
  description: string;
  profile: Omit<RemoteModelProfileDto, "api_key">;
};

const REMOTE_PRESET_STORAGE_KEY = "memori-remote-model-presets";

const REMOTE_PROVIDER_PRESETS: RemoteProviderPreset[] = [
  {
    id: "openai",
    label: "OpenAI",
    description: "Official OpenAI API, chat and embedding on one base URL.",
    profile: {
      chat_endpoint: "https://api.openai.com",
      graph_endpoint: "https://api.openai.com",
      embed_endpoint: "https://api.openai.com",
      chat_model: "gpt-4o-mini",
      graph_model: "gpt-4o-mini",
      embed_model: "text-embedding-3-small"
    }
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    description: "DeepSeek OpenAI-compatible chat endpoint.",
    profile: {
      chat_endpoint: "https://api.deepseek.com",
      graph_endpoint: "https://api.deepseek.com",
      embed_endpoint: "https://api.deepseek.com",
      chat_model: "deepseek-chat",
      graph_model: "deepseek-chat",
      embed_model: "text-embedding-3-small"
    }
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    description: "One OpenAI-compatible gateway for many hosted models.",
    profile: {
      chat_endpoint: "https://openrouter.ai/api",
      graph_endpoint: "https://openrouter.ai/api",
      embed_endpoint: "https://openrouter.ai/api",
      chat_model: "openai/gpt-4o-mini",
      graph_model: "openai/gpt-4o-mini",
      embed_model: "openai/text-embedding-3-small"
    }
  },
  {
    id: "siliconflow",
    label: "SiliconFlow",
    description: "China-hosted OpenAI-compatible model service.",
    profile: {
      chat_endpoint: "https://api.siliconflow.cn",
      graph_endpoint: "https://api.siliconflow.cn",
      embed_endpoint: "https://api.siliconflow.cn",
      chat_model: "Qwen/Qwen2.5-7B-Instruct",
      graph_model: "Qwen/Qwen2.5-7B-Instruct",
      embed_model: "BAAI/bge-m3"
    }
  },
  {
    id: "dashscope",
    label: "DashScope",
    description: "Alibaba DashScope compatible-mode endpoint.",
    profile: {
      chat_endpoint: "https://dashscope.aliyuncs.com/compatible-mode",
      graph_endpoint: "https://dashscope.aliyuncs.com/compatible-mode",
      embed_endpoint: "https://dashscope.aliyuncs.com/compatible-mode",
      chat_model: "qwen-plus",
      graph_model: "qwen-plus",
      embed_model: "text-embedding-v3"
    }
  },
  {
    id: "moonshot",
    label: "Moonshot / Kimi",
    description: "Moonshot AI OpenAI-compatible endpoint.",
    profile: {
      chat_endpoint: "https://api.moonshot.cn",
      graph_endpoint: "https://api.moonshot.cn",
      embed_endpoint: "https://api.moonshot.cn",
      chat_model: "moonshot-v1-8k",
      graph_model: "moonshot-v1-8k",
      embed_model: "text-embedding-3-small"
    }
  }
];

function applyRemotePreset(
  current: RemoteModelProfileDto,
  preset: RemoteProviderPreset
): RemoteModelProfileDto {
  return {
    ...current,
    ...preset.profile
  };
}

function parseRemotePresets(raw: string | null): RemoteProviderPreset[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is RemoteProviderPreset =>
      typeof item?.id === "string" &&
      typeof item?.label === "string" &&
      typeof item?.description === "string" &&
      typeof item?.profile?.chat_endpoint === "string" &&
      typeof item?.profile?.graph_endpoint === "string" &&
      typeof item?.profile?.embed_endpoint === "string" &&
      typeof item?.profile?.chat_model === "string" &&
      typeof item?.profile?.graph_model === "string" &&
      typeof item?.profile?.embed_model === "string"
    );
  } catch {
    return [];
  }
}

const PERFORMANCE_PRESETS: Array<{
  value: LocalPerformancePreset;
  label: string;
  description: string;
}> = [
  {
    value: "compat",
    label: "兼容模式",
    description: "不额外添加激进参数，适合不确定硬件或首次配置。"
  },
  {
    value: "gpu",
    label: "GPU 加速",
    description: "尽量把模型层放到显卡，适合显存充足的电脑。"
  },
  {
    value: "low_vram",
    label: "低显存",
    description: "降低 batch 并使用较省显存的 KV cache，适合显存紧张。"
  },
  {
    value: "throughput",
    label: "高吞吐",
    description: "提高 batch，适合显存较大且希望更快回答。"
  }
];

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

type RoleErrorMap = Partial<Record<ModelRoleKey, string>>;

function roleEndpoint(profile: LocalModelProfileDto, role: ModelRoleKey): string {
  if (role === "chat") return profile.chat_endpoint;
  if (role === "graph") return profile.graph_endpoint;
  return profile.embed_endpoint;
}

function roleModel(profile: LocalModelProfileDto, role: ModelRoleKey): string {
  if (role === "chat") return profile.chat_model;
  if (role === "graph") return profile.graph_model;
  return profile.embed_model;
}

function endpointHasUsablePort(endpoint: string): boolean {
  try {
    const url = new URL(endpoint);
    return Boolean(url.port || url.protocol === "http:" || url.protocol === "https:");
  } catch {
    return false;
  }
}

/** 返回 endpoint 的 host:port 目标标识，用于判断两个角色是否落在同一服务上。 */
function endpointTarget(endpoint: string): string | null {
  try {
    const url = new URL(endpoint.trim());
    const port = url.port || (url.protocol === "https:" ? "443" : "80");
    return `${url.hostname.toLowerCase()}:${port}`;
  } catch {
    return null;
  }
}

function optionalNumber(value: string, min?: number): number | null {
  if (value.trim() === "") return null;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return null;
  return min == null ? parsed : Math.max(min, parsed);
}

function validateLocalRoles(
  profile: LocalModelProfileDto,
  roles: readonly ModelRoleKey[]
): { ok: boolean; roleErrors: RoleErrorMap; generalErrors: string[]; firstRole: ModelRoleKey | null } {
  const roleErrors: RoleErrorMap = {};
  const generalErrors: string[] = [];

  if (!profile.llama_server_path?.trim()) {
    generalErrors.push("未选择 llama-server 可执行文件。可以继续尝试从 PATH 查找；如果启动失败，请先选择 llama-server.exe。");
  }

  for (const role of roles) {
    const label = ROLE_META[role].label;
    const modelPath = modelPathForRole(profile, role).trim();
    const modelName = roleModel(profile, role).trim();
    const endpoint = roleEndpoint(profile, role).trim();
    if (!modelPath) {
      roleErrors[role] = `${label}缺少 GGUF 文件路径，请展开卡片并点击“浏览”选择模型文件。`;
      continue;
    }
    if (!modelName) {
      roleErrors[role] = `${label}缺少模型名称。`;
      continue;
    }
    if (!endpoint || !endpointHasUsablePort(endpoint)) {
      roleErrors[role] = `${label}端口/endpoint 无效，请检查端口号。`;
    }
  }

  // 端口不可重复：一个 llama-server 进程只能服务一个角色，向量模型还需独立的 --embedding 服务。
  const seenTargets = new Map<string, ModelRoleKey>();
  for (const role of roles) {
    if (roleErrors[role]) continue;
    const target = endpointTarget(roleEndpoint(profile, role));
    if (!target) continue;
    const previous = seenTargets.get(target);
    if (previous) {
      const message = `${ROLE_META[previous].label}与${ROLE_META[role].label}使用了相同的端口（${target}）。三个角色必须使用不同的端口（默认 18001 / 18002 / 18003）。`;
      roleErrors[previous] = roleErrors[previous] ?? message;
      roleErrors[role] = message;
    } else {
      seenTargets.set(target, role);
    }
  }

  const firstRole = roles.find((role) => Boolean(roleErrors[role])) ?? null;
  return {
    ok: Object.keys(roleErrors).length === 0,
    roleErrors,
    generalErrors,
    firstRole
  };
}

function describeAvailabilityError(
  code: string,
  message: string,
  localProfile: LocalModelProfileDto | null
): string {
  if (!localProfile) return `${code}: ${message}`;
  const role = (["chat", "graph", "embed"] as const).find((candidate) => {
    const endpoint = roleEndpoint(localProfile, candidate);
    return endpoint && message.includes(endpoint);
  });
  return role ? `${ROLE_META[role].label}: ${code}: ${message}` : `${code}: ${message}`;
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

export {
  type TranslateFn,
  type ModelRoleKey,
  type RemoteProviderPreset,
  type RoleErrorMap,
  PERFORMANCE_PRESETS,
  REMOTE_PRESET_STORAGE_KEY,
  REMOTE_PROVIDER_PRESETS,
  applyRemotePreset,
  parseRemotePresets,
  extractPort,
  replacePort,
  pickModelFile,
  pickLlamaServerFile,
  fileNameFromPath,
  dirNameFromPath,
  modelPathForRole,
  runtimeStatusForRole,
  roleEndpoint,
  roleModel,
  endpointHasUsablePort,
  optionalNumber,
  validateLocalRoles,
  describeAvailabilityError,
  ROLE_META,
};
