import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { Language } from "../../i18n";
import type { AskResponseStructured } from "../types";

type UseQueryFlowParams = {
  aiLang: Language;
  retrieveTopK: number;
  selectedScopePaths: string[];
  modelSetupReady: boolean;
  onError: (message: string) => void;
  toUiErrorMessage: (error: unknown) => string;
  onSearchStart?: () => void;
  onSearchEnd?: () => void;
};

export function useQueryFlow({
  aiLang,
  retrieveTopK,
  selectedScopePaths,
  modelSetupReady,
  onError,
  toUiErrorMessage,
  onSearchStart,
  onSearchEnd
}: UseQueryFlowParams) {
  const [query, setQuery] = useState("");
  const [answerResponse, setAnswerResponse] = useState<AskResponseStructured | null>(null);
  const [loading, setLoading] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [searchElapsedMs, setSearchElapsedMs] = useState(0);
  const [lastSearchDurationMs, setLastSearchDurationMs] = useState<number | null>(null);
  const searchStartedAtRef = useRef<number | null>(null);

  useEffect(() => {
    if (!loading) {
      return;
    }
    const updateElapsed = () => {
      const startedAt = searchStartedAtRef.current;
      if (startedAt == null) {
        return;
      }
      setSearchElapsedMs(performance.now() - startedAt);
    };
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 100);
    return () => window.clearInterval(timer);
  }, [loading]);

  const runSearch = async (overrideScopePaths?: string[]) => {
    if (query.trim().length === 0 || loading || !modelSetupReady) {
      return;
    }

    setIsSearching(true);
    setLoading(true);
    setSearchElapsedMs(0);
    setLastSearchDurationMs(null);
    searchStartedAtRef.current = performance.now();
    setAnswerResponse(null);
    onSearchStart?.();

    try {
      const scopePaths = overrideScopePaths ?? selectedScopePaths;
      const response = await invoke<AskResponseStructured>("ask_vault_structured", {
        query: query.trim(),
        lang: aiLang,
        topK: retrieveTopK,
        scopePaths
      });
      setAnswerResponse(response);
    } catch (error) {
      setAnswerResponse(null);
      onError(toUiErrorMessage(error));
    } finally {
      const startedAt = searchStartedAtRef.current;
      if (startedAt != null) {
        const elapsed = performance.now() - startedAt;
        setSearchElapsedMs(elapsed);
        setLastSearchDurationMs(elapsed);
      }
      searchStartedAtRef.current = null;
      setLoading(false);
      onSearchEnd?.();
    }
  };

  return {
    query,
    setQuery,
    answerResponse,
    setAnswerResponse,
    loading,
    isSearching,
    setIsSearching,
    searchElapsedMs,
    lastSearchDurationMs,
    runSearch
  };
}
