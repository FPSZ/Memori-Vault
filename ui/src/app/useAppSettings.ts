import { open } from "@tauri-apps/plugin-dialog";
import {
  copyMcpClientConfig,
  getMcpStatus,
  listProviderModels,
  setEnterprisePolicy as saveEnterprisePolicyRemote,
  setIndexFilter as saveIndexFilterRemote,
  setMcpSettings as saveMcpSettingsRemote,
  setMemorySettings as saveMemorySettingsRemote,
  validateModelSetup
} from "./api/desktop";
import {
  INDEXING_ACTION_TIMEOUT_MS,
  MODEL_ACTION_TIMEOUT_MS,
  MODEL_NOT_CONFIGURED_CODE,
  TAURI_HOST_MISSING_MESSAGE,
  isTauriHostAvailable,
  settingsToMemorySettings,
  toUiErrorMessage,
  withTimeout
} from "./app-helpers";
import type {
  EnterprisePolicyDto,
  IndexFilterConfigDto,
  McpSettingsDto,
  MemorySettingsDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto
} from "../components/settings/types";
import type { Language } from "../i18n";

export interface UseAppSettingsDeps {
  enterprisePolicy: EnterprisePolicyDto;
  modelSettings: ModelSettingsDto;
  mcpSettings: McpSettingsDto;
  memorySettings: MemorySettingsDto;
  filterConfig: IndexFilterConfigDto;
  uiLang: Language;
  setEnterpriseBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setEnterprisePolicy: React.Dispatch<React.SetStateAction<EnterprisePolicyDto>>;
  setModelAvailability: React.Dispatch<React.SetStateAction<ModelAvailabilityDto | null>>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
  setModelSettings: React.Dispatch<React.SetStateAction<ModelSettingsDto>>;
  setProviderModels: React.Dispatch<React.SetStateAction<ProviderModelsDto>>;
  setMcpBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setMcpMessage: React.Dispatch<React.SetStateAction<string | null>>;
  setMcpSettings: React.Dispatch<React.SetStateAction<McpSettingsDto>>;
  setMcpStatus: React.Dispatch<React.SetStateAction<import("../components/settings/types").McpStatusDto | null>>;
  setMemoryBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setMemoryMessage: React.Dispatch<React.SetStateAction<string | null>>;
  setMemorySettings: React.Dispatch<React.SetStateAction<MemorySettingsDto>>;
  setFilterBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setFilterMessage: React.Dispatch<React.SetStateAction<string | null>>;
  refreshIndexingStatus: () => Promise<void>;
}

export function useAppSettings(deps: UseAppSettingsDeps) {
  const {
    enterprisePolicy,
    modelSettings,
    mcpSettings,
    memorySettings,
    filterConfig,
    uiLang,
    setEnterpriseBusy,
    setEnterprisePolicy,
    setModelAvailability,
    setError,
    setModelSettings,
    setProviderModels,
    setMcpBusy,
    setMcpMessage,
    setMcpSettings,
    setMcpStatus,
    setMemoryBusy,
    setMemoryMessage,
    setMemorySettings,
    setFilterBusy,
    setFilterMessage,
    refreshIndexingStatus
  } = deps;

  const onSaveEnterprisePolicy = async () => {
    setEnterpriseBusy(true);
    try {
      const saved = await withTimeout(
        saveEnterprisePolicyRemote(enterprisePolicy),
        MODEL_ACTION_TIMEOUT_MS,
        "Saving enterprise policy timed out."
      );
      setEnterprisePolicy(saved);
      try {
        const availability = await withTimeout(
          validateModelSetup(),
          MODEL_ACTION_TIMEOUT_MS,
          "Model validation timed out."
        );
        setModelAvailability(availability);
        if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
          setError(null);
        }
      } catch (err) {
        setModelAvailability(null);
        setError(toUiErrorMessage(err));
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setEnterpriseBusy(false);
    }
  };

  const onSelectProvider = (provider: ModelProvider) => {
    setModelAvailability(null);
    setProviderModels({ from_folder: [], from_service: [], merged: [] });
    setError(null);
    setModelSettings((prev) => ({
      ...prev,
      active_provider: provider
    }));
  };

  const onPickLocalModelsRoot = async () => {
    if (!isTauriHostAvailable()) {
      throw new Error(TAURI_HOST_MISSING_MESSAGE);
    }
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: modelSettings.local_profile.models_root || undefined
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    const next = {
      ...modelSettings,
      local_profile: {
        ...modelSettings.local_profile,
        models_root: selected
      }
    };
    setModelSettings(next);
    if (next.active_provider === "llama_cpp_local") {
      const models = await listProviderModels({
        provider: next.active_provider,
        chatEndpoint: next.local_profile.chat_endpoint,
        graphEndpoint: next.local_profile.graph_endpoint,
        embedEndpoint: next.local_profile.embed_endpoint,
        apiKey: null,
        modelsRoot: selected
      });
      setProviderModels(models);
    }
  };

  const onClearLocalModelsRoot = () => {
    setModelSettings((prev) => ({
      ...prev,
      local_profile: {
        ...prev.local_profile,
        models_root: ""
      }
    }));
    setProviderModels((prev) => ({ ...prev, from_folder: [], merged: prev.from_service }));
  };

  const onSaveMcpSettings = async () => {
    setMcpBusy(true);
    setMcpMessage(null);
    try {
      const saved = await withTimeout(
        saveMcpSettingsRemote(mcpSettings),
        MODEL_ACTION_TIMEOUT_MS,
        "Saving MCP settings timed out."
      );
      setMcpSettings(saved);
      const status = await getMcpStatus();
      setMcpStatus(status);
      setMcpMessage("MCP settings saved.");
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      setMcpMessage(message);
      throw err;
    } finally {
      setMcpBusy(false);
    }
  };

  const onCopyMcpClientConfig = async (client: string) => {
    try {
      const config = await copyMcpClientConfig(client);
      await navigator.clipboard.writeText(config);
      setMcpMessage("Client config copied.");
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      setMcpMessage(message);
    }
  };

  const onSaveMemorySettings = async () => {
    setMemoryBusy(true);
    setMemoryMessage(null);
    try {
      const saved = await withTimeout(
        saveMemorySettingsRemote(memorySettings),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存记忆设置超时，请重试。" : "Saving memory settings timed out."
      );
      setMemorySettings(settingsToMemorySettings(saved));
      setMemoryMessage(uiLang === "zh-CN" ? "记忆设置已保存。" : "Memory settings saved.");
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      setMemoryMessage(message);
      throw err;
    } finally {
      setMemoryBusy(false);
    }
  };

  const onSaveFilterConfig = async () => {
    setFilterBusy(true);
    setFilterMessage(null);
    try {
      await withTimeout(
        saveIndexFilterRemote(filterConfig),
        INDEXING_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN" ? "保存索引筛选配置超时，请重试。" : "Saving index filter timed out."
      );
      setFilterMessage(uiLang === "zh-CN" ? "索引筛选配置已保存，重新索引后生效。" : "Index filter saved. Reindex to apply it.");
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      setFilterMessage(message);
      throw err;
    } finally {
      setFilterBusy(false);
    }
  };

  return {
    onSaveEnterprisePolicy,
    onSelectProvider,
    onPickLocalModelsRoot,
    onClearLocalModelsRoot,
    onSaveMcpSettings,
    onCopyMcpClientConfig,
    onSaveMemorySettings,
    onSaveFilterConfig
  };
}
