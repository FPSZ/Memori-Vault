import { useEffect, useRef } from "react";
import { listProviderModels, searchFiles } from "./api/desktop";
import { isTauriHostAvailable, toUiErrorMessage } from "./app-helpers";
import type { ModelSettingsDto, ProviderModelsDto } from "../components/settings/types";

export interface UseAppEffectsDeps {
  isOnboardingOpen: boolean;
  isSettingsOpen: boolean;
  setIsSettingsOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setIsOnboardingOpen: React.Dispatch<React.SetStateAction<boolean>>;
  modelSettings: ModelSettingsDto;
  setProviderModels: React.Dispatch<React.SetStateAction<ProviderModelsDto>>;
  setFileMatchesOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setFileMatches: React.Dispatch<React.SetStateAction<import("./types").FileMatch[]>>;
  isSearching: boolean;
  isSearchBarCompact: boolean;
  setIsSearchBarCompact: React.Dispatch<React.SetStateAction<boolean>>;
  setIsSearchBarHovering: React.Dispatch<React.SetStateAction<boolean>>;
  setIsSearchInputFocused: React.Dispatch<React.SetStateAction<boolean>>;
  setAllowCompactHoverExpand: React.Dispatch<React.SetStateAction<boolean>>;
  isSearchInputFocused: boolean;
  scopeMenuOpen: boolean;
  query: string;
  isSearchBarCollapsed: boolean;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
  compactHoverUnlockTimerRef: React.RefObject<number | null>;
}

export function useAppEffects(deps: UseAppEffectsDeps) {
  const {
    isOnboardingOpen,
    isSettingsOpen,
    setIsSettingsOpen,
    setIsOnboardingOpen,
    modelSettings,
    setProviderModels,
    setFileMatchesOpen,
    setFileMatches,
    isSearching,
    isSearchBarCompact,
    setIsSearchBarCompact,
    setIsSearchBarHovering,
    setIsSearchInputFocused,
    setAllowCompactHoverExpand,
    isSearchInputFocused,
    scopeMenuOpen,
    query,
    isSearchBarCollapsed,
    setError,
    compactHoverUnlockTimerRef
  } = deps;

  useEffect(() => {
    const onGlobalKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key === ",") {
        event.preventDefault();
        setIsSettingsOpen((prev) => !prev);
        return;
      }

      if (event.key === "Escape" && isOnboardingOpen) {
        event.preventDefault();
        setIsOnboardingOpen(false);
        return;
      }

      if (event.key === "Escape" && isSettingsOpen) {
        event.preventDefault();
        setIsSettingsOpen(false);
      }
    };

    window.addEventListener("keydown", onGlobalKeyDown);
    return () => window.removeEventListener("keydown", onGlobalKeyDown);
  }, [isOnboardingOpen, isSettingsOpen, setIsSettingsOpen, setIsOnboardingOpen]);

  useEffect(() => {
    let cancelled = false;
    const refreshOnProviderChange = async () => {
      const profileConfigured =
        modelSettings.active_provider === "llama_cpp_local"
          ? modelSettings.local_profile.chat_endpoint.trim().length > 0 &&
            modelSettings.local_profile.graph_endpoint.trim().length > 0 &&
            modelSettings.local_profile.embed_endpoint.trim().length > 0 &&
            modelSettings.local_profile.chat_model.trim().length > 0 &&
            modelSettings.local_profile.graph_model.trim().length > 0 &&
            modelSettings.local_profile.embed_model.trim().length > 0
          : modelSettings.remote_profile.chat_endpoint.trim().length > 0 &&
            modelSettings.remote_profile.graph_endpoint.trim().length > 0 &&
            modelSettings.remote_profile.embed_endpoint.trim().length > 0 &&
            (modelSettings.remote_profile.api_key || "").trim().length > 0 &&
            modelSettings.remote_profile.chat_model.trim().length > 0 &&
            modelSettings.remote_profile.graph_model.trim().length > 0 &&
            modelSettings.remote_profile.embed_model.trim().length > 0;
      if (!profileConfigured) {
        setProviderModels({ from_folder: [], from_service: [], merged: [] });
        return;
      }
      try {
        const profile =
          modelSettings.active_provider === "llama_cpp_local"
            ? modelSettings.local_profile
            : modelSettings.remote_profile;
        const models = await listProviderModels({
          provider: modelSettings.active_provider,
          chatEndpoint: profile.chat_endpoint,
          graphEndpoint: profile.graph_endpoint,
          embedEndpoint: profile.embed_endpoint,
          apiKey:
            modelSettings.active_provider === "openai_compatible"
              ? modelSettings.remote_profile.api_key || null
              : null,
          modelsRoot:
            modelSettings.active_provider === "llama_cpp_local"
              ? modelSettings.local_profile.models_root || null
              : null
        });
        if (!cancelled) {
          setProviderModels(models);
        }
      } catch {
        // keep previous candidates; explicit refresh button still available
      }
    };

    void refreshOnProviderChange();
    return () => {
      cancelled = true;
    };
  }, [modelSettings.active_provider, setProviderModels]);

  useEffect(() => {
    if (!isSearching) {
      setIsSearchBarCompact(false);
      setIsSearchBarHovering(false);
      setIsSearchInputFocused(false);
      setAllowCompactHoverExpand(true);
    }
  }, [isSearching, setIsSearchBarCompact, setIsSearchBarHovering, setIsSearchInputFocused, setAllowCompactHoverExpand]);

  useEffect(() => {
    if (!isSearchBarCompact) {
      setIsSearchBarHovering(false);
      setAllowCompactHoverExpand(true);
    }
  }, [isSearchBarCompact, setIsSearchBarHovering, setAllowCompactHoverExpand]);

  useEffect(() => {
    return () => {
      if (compactHoverUnlockTimerRef.current !== null) {
        window.clearTimeout(compactHoverUnlockTimerRef.current);
      }
    };
  }, [compactHoverUnlockTimerRef]);

  useEffect(() => {
    if (!isTauriHostAvailable()) {
      return;
    }

    if (!isSearchInputFocused || scopeMenuOpen) {
      setFileMatchesOpen(false);
      return;
    }

    const q = query.trim();
    if (q.length < 2 || isSearchBarCollapsed) {
      setFileMatches([]);
      setFileMatchesOpen(false);
      return;
    }

    let canceled = false;
    const timer = window.setTimeout(async () => {
      try {
        const matches = await searchFiles({
          query: q,
          limit: 20,
          scopePaths: undefined
        });
        if (canceled) return;
        setFileMatches(matches);
        setFileMatchesOpen(matches.length > 0);
      } catch {
        if (canceled) return;
        setFileMatches([]);
        setFileMatchesOpen(false);
      }
    }, 70);

    return () => {
      canceled = true;
      window.clearTimeout(timer);
    };
  }, [
    isSearchBarCollapsed,
    isSearchInputFocused,
    scopeMenuOpen,
    query,
    setFileMatches,
    setFileMatchesOpen
  ]);
}
