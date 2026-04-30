import { MessageSquare, Network, Share2 } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  LocalPerformancePreset,
  LocalModelProfileDto,
  LocalModelRuntimeStatusDto,
  LocalModelRuntimeStatusesDto,
} from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];
type ModelRoleKey = "chat" | "graph" | "embed";

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
  type RoleErrorMap,
  PERFORMANCE_PRESETS,
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
