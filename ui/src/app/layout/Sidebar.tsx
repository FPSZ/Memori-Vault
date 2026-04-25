import { useMemo } from "react";
import {
  ChevronRight,
  FileText,
  FolderOpen,
  HardDrive,
  Settings as SettingsIcon,
  Sparkles
} from "lucide-react";
import { useI18n } from "../../i18n";
import type { SearchScopeItem } from "../types";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type SidebarProps = {
  t: TranslateFn;
  watchRoot: string;
  scopeItems: SearchScopeItem[];
  scopeLoading: boolean;
  expandedScopeDirs: Set<string>;
  stats: { documents: number; chunks: number; nodes: number };
  onToggleScopeDirExpanded: (path: string) => void;
  onPreviewFile: (path: string) => void;
  onToggleSettings: () => void;
};

function normalizeScopeKey(relativePath: string, fullPath: string): string {
  return relativePath || fullPath;
}

export function Sidebar({
  t,
  watchRoot,
  scopeItems,
  scopeLoading,
  expandedScopeDirs,
  stats,
  onToggleScopeDirExpanded,
  onPreviewFile,
  onToggleSettings
}: SidebarProps) {
  const rootName = watchRoot
    ? watchRoot.split(/[/\\]/).filter(Boolean).pop() || "Vault"
    : "Vault";

  // Compute keys and children counts like useScopeManager does
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

  const visibleItems = useMemo(() => {
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

  return (
    <aside className="flex h-full w-full flex-col border-r border-[var(--border-subtle)] bg-[var(--bg-surface-2)]">
      {/* Header / Logo */}
      <div className="flex h-12 items-center gap-2.5 px-4">
        <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--accent-soft)]">
          <Sparkles className="h-4 w-4 text-[var(--accent)]" />
        </div>
        <span className="text-sm font-semibold tracking-tight text-[var(--text-primary)]">
          Memori
        </span>
      </div>

      {/* Watch root */}
      <div className="mx-3 mb-2 flex items-center gap-2 rounded-lg px-2.5 py-2 text-[var(--text-secondary)]">
        <HardDrive className="h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 truncate text-xs font-medium">{rootName}</span>
      </div>

      {/* File tree */}
      <div className="flex-1 overflow-y-auto px-2 pb-2 no-scrollbar">
        {scopeLoading && (
          <div className="px-3 py-2 text-xs text-[var(--text-muted)]">{t("scopeLoading")}</div>
        )}
        {!scopeLoading && scopeItems.length === 0 && (
          <div className="px-3 py-2 text-xs text-[var(--text-muted)]">{t("scopeNoItems")}</div>
        )}
        <div className="space-y-0.5">
          {visibleItems.map((item) => (
            <SidebarTreeItem
              key={item.key}
              item={item}
              depth={item.depth}
              expandedScopeDirs={expandedScopeDirs}
              scopeChildrenCountByParentKey={scopeChildrenCountByParentKey}
              onToggleScopeDirExpanded={onToggleScopeDirExpanded}
              onPreviewFile={onPreviewFile}
            />
          ))}
        </div>
      </div>

      {/* Stats mini cards */}
      <div className="mx-3 mb-2 grid grid-cols-3 gap-1.5">
        <MiniStat value={stats.documents} label={t("docsShort")} />
        <MiniStat value={stats.chunks} label={t("chunksShort")} />
        <MiniStat value={stats.nodes} label={t("nodesShort")} />
      </div>

      {/* Footer actions */}
      <div className="border-t border-[var(--border-subtle)] px-2 py-2">
        <button
          type="button"
          onClick={onToggleSettings}
          className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-xs text-[var(--text-secondary)] transition hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
        >
          <SettingsIcon className="h-3.5 w-3.5" />
          {t("settings")}
        </button>
      </div>
    </aside>
  );
}

function MiniStat({ value, label }: { value: number; label: string }) {
  return (
    <div className="flex flex-col items-center rounded-md bg-[var(--bg-surface-1)] px-1 py-1.5">
      <span className="text-xs font-semibold text-[var(--text-primary)]">{value}</span>
      <span className="text-[10px] text-[var(--text-muted)]">{label}</span>
    </div>
  );
}

type SidebarTreeItemProps = {
  item: SearchScopeItem & { key: string };
  depth: number;
  expandedScopeDirs: Set<string>;
  scopeChildrenCountByParentKey: Map<string, number>;
  onToggleScopeDirExpanded: (path: string) => void;
  onPreviewFile: (path: string) => void;
};

function SidebarTreeItem({
  item,
  depth,
  expandedScopeDirs,
  scopeChildrenCountByParentKey,
  onToggleScopeDirExpanded,
  onPreviewFile
}: SidebarTreeItemProps) {
  const isExpanded = expandedScopeDirs.has(item.path);
  const hasChildren = item.is_dir && (scopeChildrenCountByParentKey.get(item.key) ?? 0) > 0;
  const displayName = item.name.trim() ? item.name : item.path;

  const handleClick = () => {
    if (item.is_dir) {
      onToggleScopeDirExpanded(item.path);
    } else {
      onPreviewFile(item.path);
    }
  };

  return (
    <div>
      <button
        type="button"
        onClick={handleClick}
        className="group flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left transition text-[var(--text-secondary)] hover:bg-[var(--bg-surface-1)] hover:text-[var(--text-primary)]"
        style={{ paddingLeft: `${8 + depth * 12}px` }}
      >
        {hasChildren ? (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleScopeDirExpanded(item.path);
            }}
            className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded transition hover:text-[var(--accent)]"
          >
            <ChevronRight
              className={`h-3 w-3 transition-transform ${isExpanded ? "rotate-90" : ""}`}
            />
          </button>
        ) : (
          <span className="h-4 w-4 shrink-0" />
        )}

        {item.is_dir ? (
          <FolderOpen className="h-3.5 w-3.5 shrink-0" />
        ) : (
          <FileText className="h-3.5 w-3.5 shrink-0" />
        )}

        <span className="min-w-0 truncate text-xs">{displayName}</span>
      </button>
    </div>
  );
}
