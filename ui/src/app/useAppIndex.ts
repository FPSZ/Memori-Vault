import {
  getIndexingStatus,
  pauseIndexing,
  resumeIndexing,
  setIndexingMode as saveIndexingModeRemote,
  triggerReindex
} from "./api/desktop";
import {
  INDEXING_ACTION_TIMEOUT_MS,
  normalizeIndexingMode,
  normalizeResourceBudget,
  toUiErrorMessage,
  withTimeout
} from "./app-helpers";
import type {
  IndexingMode,
  IndexingStatusDto,
  ResourceBudget
} from "../components/settings/types";

export interface UseAppIndexDeps {
  indexingMode: IndexingMode;
  resourceBudget: ResourceBudget;
  scheduleStart: string;
  scheduleEnd: string;
  setIndexingMode: React.Dispatch<React.SetStateAction<IndexingMode>>;
  setResourceBudget: React.Dispatch<React.SetStateAction<ResourceBudget>>;
  setScheduleStart: React.Dispatch<React.SetStateAction<string>>;
  setScheduleEnd: React.Dispatch<React.SetStateAction<string>>;
  setIndexingStatus: React.Dispatch<React.SetStateAction<IndexingStatusDto | null>>;
  setIndexingBusy: React.Dispatch<React.SetStateAction<boolean>>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
}

export function useAppIndex(deps: UseAppIndexDeps) {
  const {
    indexingMode,
    resourceBudget,
    scheduleStart,
    scheduleEnd,
    setIndexingMode,
    setResourceBudget,
    setScheduleStart,
    setScheduleEnd,
    setIndexingStatus,
    setIndexingBusy,
    setError
  } = deps;

  const refreshIndexingStatus = async () => {
    const status = await withTimeout(
      getIndexingStatus(),
      INDEXING_ACTION_TIMEOUT_MS,
      "Fetching indexing status timed out."
    );
    setIndexingStatus({
      ...status,
      mode: normalizeIndexingMode(status.mode),
      resource_budget: normalizeResourceBudget(status.resource_budget)
    });
  };

  const onSaveIndexingConfig = async () => {
    setIndexingBusy(true);
    try {
      const saved = await withTimeout(
        saveIndexingModeRemote({
          indexing_mode: indexingMode,
          resource_budget: resourceBudget,
          schedule_start: indexingMode === "scheduled" ? scheduleStart : null,
          schedule_end: indexingMode === "scheduled" ? scheduleEnd : null
        }),
        INDEXING_ACTION_TIMEOUT_MS,
        "Saving indexing config timed out."
      );
      setIndexingMode(normalizeIndexingMode(saved.indexing_mode));
      setResourceBudget(normalizeResourceBudget(saved.resource_budget));
      setScheduleStart(saved.schedule_start || "00:00");
      setScheduleEnd(saved.schedule_end || "06:00");
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onTriggerReindex = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        triggerReindex(),
        INDEXING_ACTION_TIMEOUT_MS * 2,
        "Triggering reindex timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onPauseIndexing = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        pauseIndexing(),
        INDEXING_ACTION_TIMEOUT_MS,
        "Pausing indexing timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  const onResumeIndexing = async () => {
    setIndexingBusy(true);
    try {
      await withTimeout(
        resumeIndexing(),
        INDEXING_ACTION_TIMEOUT_MS,
        "Resuming indexing timed out."
      );
      await refreshIndexingStatus();
    } catch (err) {
      const message = toUiErrorMessage(err);
      setError(message);
      throw err;
    } finally {
      setIndexingBusy(false);
    }
  };

  return {
    refreshIndexingStatus,
    onSaveIndexingConfig,
    onTriggerReindex,
    onPauseIndexing,
    onResumeIndexing
  };
}
