import { open } from "@tauri-apps/plugin-dialog";
import {
  getVaultStats,
  openSourceLocation,
  readFilePreview,
  setWatchRoot as saveWatchRootRemote
} from "./api/desktop";
import {
  MAX_SIDEBAR_WIDTH,
  MIN_SIDEBAR_WIDTH,
  SIDEBAR_WIDTH_STORAGE_KEY,
  TAURI_HOST_MISSING_MESSAGE,
  isTauriHostAvailable,
  normalizeStats,
  toUiErrorMessage
} from "./app-helpers";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  UIEvent as ReactUIEvent,
  WheelEvent as ReactWheelEvent
} from "react";

export interface UseAppUIDeps {
  setThemeMode: React.Dispatch<React.SetStateAction<import("../components/settings/types").ThemeMode>>;
  runSearch: () => Promise<void>;
  setModelSettings: React.Dispatch<React.SetStateAction<import("../components/settings/types").ModelSettingsDto>>;
  setExpandedSourceKeys: React.Dispatch<React.SetStateAction<Set<string>>>;
  setExpandedCitationKeys: React.Dispatch<React.SetStateAction<Set<string>>>;
  setError: React.Dispatch<React.SetStateAction<string | null>>;
  setPreviewFilePath: React.Dispatch<React.SetStateAction<string | null>>;
  setPreviewContent: React.Dispatch<React.SetStateAction<string | null>>;
  setPreviewFormat: React.Dispatch<React.SetStateAction<string>>;
  setIsPickingWatchRoot: React.Dispatch<React.SetStateAction<boolean>>;
  watchRoot: string;
  setWatchRoot: React.Dispatch<React.SetStateAction<string>>;
  setSelectedScopePaths: React.Dispatch<React.SetStateAction<string[]>>;
  setExpandedScopeDirs: React.Dispatch<React.SetStateAction<Set<string>>>;
  setStats: React.Dispatch<React.SetStateAction<import("./types").VaultStats>>;
  isSearchBarCompact: boolean;
  allowCompactHoverExpand: boolean;
  scopeMenuOpen: boolean;
  isSearchBarHovering: boolean;
  isSearchInputFocused: boolean;
  setIsSearchBarCompact: React.Dispatch<React.SetStateAction<boolean>>;
  setAllowCompactHoverExpand: React.Dispatch<React.SetStateAction<boolean>>;
  setIsSearchBarHovering: React.Dispatch<React.SetStateAction<boolean>>;
  setIsSearchInputFocused: React.Dispatch<React.SetStateAction<boolean>>;
  setScopeMenuOpen: React.Dispatch<React.SetStateAction<boolean>>;
  searchInputRef: React.RefObject<HTMLInputElement | null>;
  compactHoverUnlockTimerRef: React.RefObject<number | null>;
  reachedTopWhileCompactRef: React.RefObject<boolean>;
  sidebarWidthRef: React.RefObject<number>;
  setSidebarWidth: React.Dispatch<React.SetStateAction<number>>;
}

export function useAppUI(deps: UseAppUIDeps) {
  const {
    setThemeMode,
    runSearch,
    setModelSettings,
    setExpandedSourceKeys,
    setExpandedCitationKeys,
    setError,
    setPreviewFilePath,
    setPreviewContent,
    setPreviewFormat,
    setIsPickingWatchRoot,
    watchRoot,
    setWatchRoot,
    setSelectedScopePaths,
    setExpandedScopeDirs,
    setStats,
    isSearchBarCompact,
    allowCompactHoverExpand,
    scopeMenuOpen,
    isSearchBarHovering,
    isSearchInputFocused,
    setIsSearchBarCompact,
    setAllowCompactHoverExpand,
    setIsSearchBarHovering,
    setIsSearchInputFocused,
    setScopeMenuOpen,
    searchInputRef,
    compactHoverUnlockTimerRef,
    reachedTopWhileCompactRef,
    sidebarWidthRef,
    setSidebarWidth
  } = deps;

  const onToggleThemeMode = () => {
    setThemeMode((prev) => (prev === "dark" ? "light" : "dark"));
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void runSearch();
    }
  };

  const updateActiveOnboardingProfile = (
    patch: Partial<{
      chat_endpoint: string;
      graph_endpoint: string;
      embed_endpoint: string;
      api_key?: string | null;
      chat_model: string;
      graph_model: string;
      embed_model: string;
    }>
  ) => {
    setModelSettings((prev) => {
      if (prev.active_provider === "llama_cpp_local") {
        return {
          ...prev,
          local_profile: { ...prev.local_profile, ...patch }
        };
      }
      return {
        ...prev,
        remote_profile: { ...prev.remote_profile, ...patch }
      };
    });
  };

  const toggleSourceExpanded = (key: string) => {
    setExpandedSourceKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const toggleCitationExpanded = (key: string) => {
    setExpandedCitationKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const onOpenSourceLocation = async (path: string) => {
    try {
      await openSourceLocation(path);
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onPreviewFile = async (path: string) => {
    if (!path) return;
    try {
      const preview = await readFilePreview(path);
      setPreviewFilePath(path);
      setPreviewContent(preview.content);
      setPreviewFormat(preview.format);
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onCloseFilePreview = () => {
    setPreviewFilePath(null);
    setPreviewContent(null);
    setPreviewFormat("text");
  };

  const onPickWatchRoot = async () => {
    if (!isTauriHostAvailable()) {
      setError(TAURI_HOST_MISSING_MESSAGE);
      return;
    }

    setIsPickingWatchRoot(true);
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: watchRoot || undefined
      });

      if (!selected || Array.isArray(selected)) {
        return;
      }

      const settings = await saveWatchRootRemote(selected);
      setWatchRoot(settings.watch_root ?? selected);
      setSelectedScopePaths([]);
      setExpandedScopeDirs(new Set());

      const raw = await getVaultStats();
      setStats(normalizeStats(raw));
      setError(null);
    } catch (err) {
      setError(toUiErrorMessage(err));
    } finally {
      setIsPickingWatchRoot(false);
    }
  };

  const onResultScroll = (event: ReactUIEvent<HTMLElement>) => {
    const scrollTop = event.currentTarget.scrollTop;
    const shouldCompact = scrollTop > 2;
    if (shouldCompact) {
      reachedTopWhileCompactRef.current = false;
      if (allowCompactHoverExpand) {
        setAllowCompactHoverExpand(false);
      }
      if (compactHoverUnlockTimerRef.current !== null) {
        window.clearTimeout(compactHoverUnlockTimerRef.current);
      }
      compactHoverUnlockTimerRef.current = window.setTimeout(() => {
        setAllowCompactHoverExpand(true);
      }, 260);
      if (scopeMenuOpen) setScopeMenuOpen(false);
      if (isSearchBarHovering) setIsSearchBarHovering(false);
      if (isSearchInputFocused) {
        setIsSearchInputFocused(false);
        searchInputRef.current?.blur();
      }
      setIsSearchBarCompact((prev) => (prev === shouldCompact ? prev : shouldCompact));
      return;
    }
    reachedTopWhileCompactRef.current = true;
  };

  const onResultWheel = (event: ReactWheelEvent<HTMLElement>) => {
    if (!isSearchBarCompact) {
      return;
    }
    if (event.deltaY < 0 && event.currentTarget.scrollTop <= 2 && reachedTopWhileCompactRef.current) {
      reachedTopWhileCompactRef.current = false;
      setIsSearchBarCompact(false);
    }
  };

  const onSidebarResizeStart = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidthRef.current ?? 260;
    const onMouseMove = (moveEvent: MouseEvent) => {
      const delta = moveEvent.clientX - startX;
      const newWidth = Math.max(MIN_SIDEBAR_WIDTH, Math.min(MAX_SIDEBAR_WIDTH, startWidth + delta));
      setSidebarWidth(newWidth);
    };
    const onMouseUp = () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
      window.localStorage.setItem(SIDEBAR_WIDTH_STORAGE_KEY, String(sidebarWidthRef.current ?? 260));
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  };

  return {
    onToggleThemeMode,
    onKeyDown,
    updateActiveOnboardingProfile,
    toggleSourceExpanded,
    toggleCitationExpanded,
    onOpenSourceLocation,
    onPreviewFile,
    onCloseFilePreview,
    onPickWatchRoot,
    onResultScroll,
    onResultWheel,
    onSidebarResizeStart
  };
}
