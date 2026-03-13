import { useEffect, useMemo, useRef, useState } from "react";
import { listSearchScopes } from "../api/desktop";
import { normalizeScopeKey } from "../formatters";
import type { SearchScopeItem } from "../types";

type UseScopeManagerOptions = {
  watchRoot: string;
  toUiErrorMessage: (error: unknown) => string;
  onError: (message: string) => void;
};

function isTauriHostAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const w = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

export function useScopeManager({ watchRoot, toUiErrorMessage, onError }: UseScopeManagerOptions) {
  const [scopeMenuOpen, setScopeMenuOpen] = useState(false);
  const [scopeItems, setScopeItems] = useState<SearchScopeItem[]>([]);
  const [scopeLoading, setScopeLoading] = useState(false);
  const [selectedScopePaths, setSelectedScopePaths] = useState<string[]>([]);
  const [expandedScopeDirs, setExpandedScopeDirs] = useState<Set<string>>(() => new Set());
  const scopeMenuRef = useRef<HTMLDivElement | null>(null);

  const selectedScopeSet = useMemo(() => new Set(selectedScopePaths), [selectedScopePaths]);

  const scopeViewItems = useMemo(() => {
    return scopeItems.map((item) => {
      const key = normalizeScopeKey(item.relative_path ?? "", item.path);
      const slashIndex = key.lastIndexOf("/");
      const parentKey = slashIndex >= 0 ? key.slice(0, slashIndex) : "";
      return { ...item, key, parentKey };
    });
  }, [scopeItems]);

  const scopeChildrenCountByParentKey = useMemo(() => {
    const counts = new Map<string, number>();
    for (const item of scopeViewItems) {
      const prev = counts.get(item.parentKey) ?? 0;
      counts.set(item.parentKey, prev + 1);
    }
    return counts;
  }, [scopeViewItems]);

  const visibleScopeItems = useMemo(() => {
    const byKey = new Map(scopeViewItems.map((item) => [item.key, item] as const));
    const visibilityMemo = new Map<string, boolean>();

    const isVisible = (item: (typeof scopeViewItems)[number]): boolean => {
      if (visibilityMemo.has(item.key)) {
        return visibilityMemo.get(item.key) ?? false;
      }
      if (!item.parentKey) {
        visibilityMemo.set(item.key, true);
        return true;
      }

      const parent = byKey.get(item.parentKey);
      if (!parent || !parent.is_dir) {
        visibilityMemo.set(item.key, true);
        return true;
      }

      const parentVisible = isVisible(parent);
      const visible = parentVisible && expandedScopeDirs.has(parent.path);
      visibilityMemo.set(item.key, visible);
      return visible;
    };

    return scopeViewItems.filter((item) => isVisible(item));
  }, [expandedScopeDirs, scopeViewItems]);

  useEffect(() => {
    let active = true;

    const loadScopes = async () => {
      if (!isTauriHostAvailable()) {
        return;
      }
      setScopeLoading(true);
      try {
        const scopes = await listSearchScopes();
        if (!active) {
          return;
        }
        setScopeItems(scopes);
        setSelectedScopePaths((prev) => prev.filter((path) => scopes.some((s) => s.path === path)));
        setExpandedScopeDirs(
          (prev) =>
            new Set(
              [...prev].filter((path) => scopes.some((s) => s.is_dir && s.path === path))
            )
        );
      } catch (err) {
        if (active) {
          onError(toUiErrorMessage(err));
        }
      } finally {
        if (active) {
          setScopeLoading(false);
        }
      }
    };

    void loadScopes();
    return () => {
      active = false;
    };
  }, [onError, toUiErrorMessage, watchRoot]);

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!scopeMenuOpen) {
        return;
      }
      if (!scopeMenuRef.current) {
        return;
      }
      const target = event.target as Node | null;
      if (target && !scopeMenuRef.current.contains(target)) {
        setScopeMenuOpen(false);
      }
    };

    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [scopeMenuOpen]);

  const onToggleScopePath = (path: string) => {
    setSelectedScopePaths((prev) => {
      if (prev.includes(path)) {
        return prev.filter((p) => p !== path);
      }
      return [...prev, path];
    });
  };

  const onClearScopeSelection = () => {
    setSelectedScopePaths([]);
  };

  const onToggleScopeDirExpanded = (path: string) => {
    setExpandedScopeDirs((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  return {
    scopeMenuOpen,
    setScopeMenuOpen,
    scopeMenuRef,
    scopeItems,
    scopeLoading,
    selectedScopePaths,
    setSelectedScopePaths,
    selectedScopeSet,
    expandedScopeDirs,
    setExpandedScopeDirs,
    scopeChildrenCountByParentKey,
    visibleScopeItems,
    onToggleScopePath,
    onClearScopeSelection,
    onToggleScopeDirExpanded
  };
}
