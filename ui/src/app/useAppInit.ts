import { useEffect } from "react";
import {
  getAppSettings,
  getEnterprisePolicy,
  getIndexFilter,
  getIndexingStatus,
  getLocalModelRuntimeStatus,
  getMcpSettings,
  getMcpStatus,
  getModelSettings,
  getVaultStats,
  listProviderModels,
  validateModelSetup
} from "./api/desktop";
import {
  AI_LANG_STORAGE_KEY,
  DEFAULT_FILTER_CONFIG,
  MODEL_NOT_CONFIGURED_CODE,
  normalizeIndexingMode,
  normalizeResourceBudget,
  normalizeStats,
  settingsToMemorySettings,
  toUiErrorMessage
} from "./app-helpers";
import type {
  EnterprisePolicyDto,
  IndexFilterConfigDto,
  IndexingMode,
  IndexingStatusDto,
  LocalModelRuntimeStatusesDto,
  McpSettingsDto,
  McpStatusDto,
  MemorySettingsDto,
  ModelAvailabilityDto,
  ModelSettingsDto,
  ProviderModelsDto,
  ResourceBudget,
  ThemeMode
} from "../components/settings/types";
import type { Language } from "../i18n";
import type { VaultStats } from "./types";

export interface UseAppInitDeps {
  setStats: React.Dispatch<React.SetStateAction<VaultStats>>;
  setWatchRoot: React.Dispatch<React.SetStateAction<string>>;
  setIndexingMode: React.Dispatch<React.SetStateAction<IndexingMode>>;
  setResourceBudget: React.Dispatch<React.SetStateAction<ResourceBudget>>;
  setScheduleStart: React.Dispatch<React.SetStateAction<string>>;
  setScheduleEnd: React.Dispatch<React.SetStateAction<string>>;
  setMemorySettings: React.Dispatch<React.SetStateAction<MemorySettingsDto>>;
  setFilterConfig: React.Dispatch<React.SetStateAction<IndexFilterConfigDto>>;
  setAiLang: React.Dispatch<React.SetStateAction<Language>>;
  setEnterprisePolicy: React.Dispatch<React.SetStateAction<EnterprisePolicyDto>>;
  setMcpSettings: React.Dispatch<React.SetStateAction<McpSettingsDto>>;
  setMcpStatus: React.Dispatch<React.SetStateAction<McpStatusDto | null>>;
  setModelSettings: React.Dispatch<React.SetStateAction<ModelSettingsDto>>;
  setProviderModels: React.Dispatch<React.SetStateAction<ProviderModelsDto>>;
  setLocalModelRuntimeStatuses: React.Dispatch<React.SetStateAction<LocalModelRuntimeStatusesDto | null>>;
  setModelAvailability: React.Dispatch<React.SetStateAction<ModelAvailabilityDto | null>>;
  setIndexingStatus: React.Dispatch<React.SetStateAction<IndexingStatusDto | null>>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
}

export function useAppInit(deps: UseAppInitDeps) {
  const {
    setStats,
    setWatchRoot,
    setIndexingMode,
    setResourceBudget,
    setScheduleStart,
    setScheduleEnd,
    setMemorySettings,
    setFilterConfig,
    setAiLang,
    setEnterprisePolicy,
    setMcpSettings,
    setMcpStatus,
    setModelSettings,
    setProviderModels,
    setLocalModelRuntimeStatuses,
    setModelAvailability,
    setIndexingStatus,
    setError
  } = deps;

  useEffect(() => {
    let active = true;

    const loadStats = async () => {
      try {
        const raw = await getVaultStats();
        if (active) {
          setStats(normalizeStats(raw));
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadSettings = async () => {
      try {
        const settings = await getAppSettings();
        if (active) {
          setWatchRoot(settings.watch_root ?? "");
          setIndexingMode(normalizeIndexingMode(settings.indexing_mode));
          setResourceBudget(normalizeResourceBudget(settings.resource_budget));
          setScheduleStart(settings.schedule_start || "00:00");
          setScheduleEnd(settings.schedule_end || "06:00");
          setMemorySettings(settingsToMemorySettings(settings));
          try {
            const filter = await getIndexFilter();
            if (active && filter) {
              setFilterConfig(filter);
            }
          } catch {
            // ignore filter loading failure
          }
          if (!window.localStorage.getItem(AI_LANG_STORAGE_KEY) && settings.language) {
            const normalized = settings.language.toLowerCase();
            if (normalized.startsWith("zh")) {
              setAiLang("zh-CN");
            } else if (normalized.startsWith("en")) {
              setAiLang("en-US");
            }
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadIndexingStatus = async () => {
      try {
        const status = await getIndexingStatus();
        if (active) {
          setIndexingStatus({
            ...status,
            mode: normalizeIndexingMode(status.mode),
            resource_budget: normalizeResourceBudget(status.resource_budget)
          });
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadIndexFilter = async () => {
      try {
        const config = await getIndexFilter();
        if (active) {
          setFilterConfig(config ?? DEFAULT_FILTER_CONFIG);
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadModelSettings = async () => {
      try {
        const settings = await getModelSettings();
        if (active) {
          setModelSettings(settings);
          getLocalModelRuntimeStatus()
            .then((runtime) => {
              if (active) setLocalModelRuntimeStatuses(runtime);
            })
            .catch(() => {
              if (active) setLocalModelRuntimeStatuses(null);
            });
        }
        const profileConfigured =
          settings.active_provider === "llama_cpp_local"
            ? settings.local_profile.chat_endpoint.trim().length > 0 &&
              settings.local_profile.graph_endpoint.trim().length > 0 &&
              settings.local_profile.embed_endpoint.trim().length > 0 &&
              settings.local_profile.chat_model.trim().length > 0 &&
              settings.local_profile.graph_model.trim().length > 0 &&
              settings.local_profile.embed_model.trim().length > 0
            : settings.remote_profile.chat_endpoint.trim().length > 0 &&
              settings.remote_profile.graph_endpoint.trim().length > 0 &&
              settings.remote_profile.embed_endpoint.trim().length > 0 &&
              (settings.remote_profile.api_key || "").trim().length > 0 &&
              settings.remote_profile.chat_model.trim().length > 0 &&
              settings.remote_profile.graph_model.trim().length > 0 &&
              settings.remote_profile.embed_model.trim().length > 0;
        if (!profileConfigured) {
          if (active) {
            setProviderModels({ from_folder: [], from_service: [], merged: [] });
          }
          return;
        }

        try {
          const profile =
            settings.active_provider === "llama_cpp_local"
              ? settings.local_profile
              : settings.remote_profile;
          const models = await listProviderModels({
            provider: settings.active_provider,
            chatEndpoint: profile.chat_endpoint,
            graphEndpoint: profile.graph_endpoint,
            embedEndpoint: profile.embed_endpoint,
            apiKey:
              settings.active_provider === "openai_compatible"
                ? settings.remote_profile.api_key || null
                : null,
            modelsRoot:
              settings.active_provider === "llama_cpp_local"
                ? settings.local_profile.models_root || null
                : null
          });
          if (active) {
            setProviderModels(models);
          }
        } catch {
          if (active) {
            setProviderModels({ from_folder: [], from_service: [], merged: [] });
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadEnterprisePolicy = async () => {
      try {
        const policy = await getEnterprisePolicy();
        if (active) {
          setEnterprisePolicy(policy);
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadMcpSettings = async () => {
      try {
        const [settings, status] = await Promise.all([getMcpSettings(), getMcpStatus()]);
        if (active) {
          setMcpSettings(settings);
          setMcpStatus(status);
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    const loadModelAvailability = async () => {
      try {
        const availability = await validateModelSetup();
        if (active) {
          setModelAvailability(availability);
          if (availability.status_code === MODEL_NOT_CONFIGURED_CODE) {
            setError(null);
          }
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    void loadStats();
    void loadSettings();
    void loadIndexingStatus();
    void loadIndexFilter();
    void loadEnterprisePolicy();
    void loadMcpSettings();
    void loadModelSettings().then(() => {
      void loadModelAvailability();
    });
    const timer = window.setInterval(() => {
      void loadStats();
      void loadIndexingStatus();
    }, 5000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [
    setStats,
    setWatchRoot,
    setIndexingMode,
    setResourceBudget,
    setScheduleStart,
    setScheduleEnd,
    setMemorySettings,
    setFilterConfig,
    setAiLang,
    setEnterprisePolicy,
    setMcpSettings,
    setMcpStatus,
    setModelSettings,
    setProviderModels,
    setLocalModelRuntimeStatuses,
    setModelAvailability,
    setIndexingStatus,
    setError
  ]);
}
