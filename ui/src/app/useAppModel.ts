import {
  getLocalModelRuntimeStatus,
  listProviderModels,
  probeModelProvider,
  restartLocalModel,
  setModelSettings as saveModelSettingsRemote,
  startLocalModel,
  stopLocalModel,
  validateModelSetup
} from "./api/desktop";
import {
  LOCAL_MODEL_ACTION_TIMEOUT_MS,
  MODEL_ACTION_TIMEOUT_MS,
  MODEL_NOT_CONFIGURED_CODE,
  toUiErrorMessage,
  withTimeout
} from "./app-helpers";
import type {
  LocalModelRuntimeStatusesDto,
  ModelAvailabilityDto,
  ModelSettingsDto,
  ProviderModelsDto
} from "../components/settings/types";
import type { Language } from "../i18n";

export interface UseAppModelDeps {
  modelSettings: ModelSettingsDto;
  activeModelProfile: {
    chat_endpoint: string;
    graph_endpoint: string;
    embed_endpoint: string;
    chat_model: string;
    graph_model: string;
    embed_model: string;
    models_root?: string | null;
    api_key?: string | null;
  };
  uiLang: Language;
  setModelBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
  setModelAvailability: React.Dispatch<React.SetStateAction<ModelAvailabilityDto | null>>;
  setProviderModels: React.Dispatch<React.SetStateAction<ProviderModelsDto>>;
  setModelSettings: React.Dispatch<React.SetStateAction<ModelSettingsDto>>;
  setLocalModelRuntimeStatuses: React.Dispatch<React.SetStateAction<LocalModelRuntimeStatusesDto | null>>;
  setLocalModelRuntimeBusyRole: React.Dispatch<React.SetStateAction<string | null>>;
  setIsOnboardingOpen: React.Dispatch<React.SetStateAction<boolean>>;
}

export function useAppModel(deps: UseAppModelDeps) {
  const {
    modelSettings,
    activeModelProfile,
    uiLang,
    setModelBusy,
    setError,
    setModelAvailability,
    setProviderModels,
    setModelSettings,
    setLocalModelRuntimeStatuses,
    setLocalModelRuntimeBusyRole,
    setIsOnboardingOpen
  } = deps;

  const onProbeModelProvider = async () => {
    setModelBusy(true);
    try {
      const availability = await withTimeout(
        probeModelProvider({
          provider: modelSettings.active_provider,
          chatEndpoint: activeModelProfile.chat_endpoint,
          graphEndpoint: activeModelProfile.graph_endpoint,
          embedEndpoint: activeModelProfile.embed_endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "llama_cpp_local"
              ? modelSettings.local_profile.models_root || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS,
        uiLang === "zh-CN"
          ? "Model provider request timed out. Please check endpoint/network."
          : "Model provider request timed out. Please check endpoint/network."
      );
      setModelAvailability(availability);
      if (!availability.reachable) {
        const first = availability.errors?.[0];
        throw new Error(
          first ? `${first.code}: ${first.message}` : uiLang === "zh-CN" ? "连接失败" : "Connection failed"
        );
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onRefreshProviderModels = async () => {
    setModelBusy(true);
    try {
      const models = await withTimeout(
        listProviderModels({
          provider: modelSettings.active_provider,
          chatEndpoint: activeModelProfile.chat_endpoint,
          graphEndpoint: activeModelProfile.graph_endpoint,
          embedEndpoint: activeModelProfile.embed_endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "llama_cpp_local"
              ? modelSettings.local_profile.models_root || null
              : null
        }),
        MODEL_ACTION_TIMEOUT_MS,
        "Refreshing model list timed out."
      );
      setProviderModels(models);
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onSaveModelSettings = async () => {
    setModelBusy(true);
    try {
      const saved = await withTimeout(
        saveModelSettingsRemote(modelSettings),
        MODEL_ACTION_TIMEOUT_MS,
        "Saving model settings timed out."
      );
      setModelSettings(saved);
      try {
        const runtime = await getLocalModelRuntimeStatus();
        setLocalModelRuntimeStatuses(runtime);
      } catch {
        setLocalModelRuntimeStatuses(null);
      }
      const availability = await withTimeout(
        validateModelSetup(),
        MODEL_ACTION_TIMEOUT_MS,
        "Model validation timed out."
      );
      setModelAvailability(availability);
      if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
        setProviderModels({ from_folder: [], from_service: [], merged: [] });
        setError(null);
      } else {
        const refreshedModels = await withTimeout(
          listProviderModels({
            provider: saved.active_provider,
            chatEndpoint:
              saved.active_provider === "llama_cpp_local"
                ? saved.local_profile.chat_endpoint
                : saved.remote_profile.chat_endpoint,
            graphEndpoint:
              saved.active_provider === "llama_cpp_local"
                ? saved.local_profile.graph_endpoint
                : saved.remote_profile.graph_endpoint,
            embedEndpoint:
              saved.active_provider === "llama_cpp_local"
                ? saved.local_profile.embed_endpoint
                : saved.remote_profile.embed_endpoint,
            apiKey:
              saved.active_provider === "openai_compatible"
                ? saved.remote_profile.api_key || null
                : null,
            modelsRoot:
              saved.active_provider === "llama_cpp_local" ? saved.local_profile.models_root || null : null
          }),
          MODEL_ACTION_TIMEOUT_MS,
          "Refreshing model list timed out."
        );
        setProviderModels(refreshedModels);
      }
      if (availability.reachable && (availability.missing_roles?.length ?? 0) === 0) {
        setIsOnboardingOpen(false);
        setError(null);
      }
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setModelBusy(false);
    }
  };

  const onRefreshLocalModelRuntimeStatus = async () => {
    try {
      const runtime = await getLocalModelRuntimeStatus();
      setLocalModelRuntimeStatuses(runtime);
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    }
  };

  const runLocalModelRuntimeAction = async (
    role: "chat" | "graph" | "embed",
    action: () => Promise<LocalModelRuntimeStatusesDto>
  ) => {
    setLocalModelRuntimeBusyRole(role);
    try {
      const saved = await withTimeout(
        saveModelSettingsRemote(modelSettings),
        MODEL_ACTION_TIMEOUT_MS,
        "Saving model settings timed out."
      );
      setModelSettings(saved);
      const runtime = await withTimeout(
        action(),
        LOCAL_MODEL_ACTION_TIMEOUT_MS,
        "Local model runtime action timed out."
      );
      setLocalModelRuntimeStatuses(runtime);
      setError(null);
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      try {
        const runtime = await getLocalModelRuntimeStatus();
        setLocalModelRuntimeStatuses(runtime);
      } catch {
        // Keep the original action error visible.
      }
      throw err;
    } finally {
      setLocalModelRuntimeBusyRole(null);
    }
  };

  const onStartLocalModel = (role: "chat" | "graph" | "embed") =>
    runLocalModelRuntimeAction(role, () => startLocalModel(role));

  const onStopLocalModel = (role: "chat" | "graph" | "embed") =>
    runLocalModelRuntimeAction(role, () => stopLocalModel(role));

  const onRestartLocalModel = (role: "chat" | "graph" | "embed") =>
    runLocalModelRuntimeAction(role, () => restartLocalModel(role));

  return {
    onProbeModelProvider,
    onRefreshProviderModels,
    onSaveModelSettings,
    onRefreshLocalModelRuntimeStatus,
    onStartLocalModel,
    onStopLocalModel,
    onRestartLocalModel
  };
}
