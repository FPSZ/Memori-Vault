import { motion } from "framer-motion";
import { ChevronDown, ChevronRight, FileText, Folder, RefreshCw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { listSearchScopes } from "../../../app/api/desktop";
import type { SearchScopeItem } from "../../../app/types";
import { useI18n } from "../../../i18n";
import { AnimatedPanel, AnimatedPressButton } from "../../MotionKit";
import { CyberToggle } from "../../UI";
import type { IndexFilterConfigDto } from "../types";

type FilterPanelProps = {
  filterConfig: IndexFilterConfigDto;
  filterBusy: boolean;
  filterMessage: string | null;
  watchRoot?: string;
  initiallyOpen?: boolean;
  onFilterConfigChange: (next: IndexFilterConfigDto) => void;
  onSaveFilterConfig: () => Promise<void>;
};

type RuleMode = "include" | "exclude";

const SUPPORTED_EXTENSIONS = ["md", "txt", "pdf", "docx"];

function uniq(values: string[]): string[] {
  return Array.from(new Set(values.map((value) => value.trim()).filter(Boolean)));
}

function normalizeExt(value: string): string {
  return value.trim().replace(/^\./, "").toLowerCase();
}

function removeValue(values: string[], value: string): string[] {
  return values.filter((item) => item !== value);
}

function scopeItemToRule(item: SearchScopeItem): string {
  const relative = (item.relative_path || item.name || item.path).replace(/\\/g, "/").replace(/\/$/, "");
  if (!relative) return "**";
  if (!item.is_dir) return relative;
  return relative.endsWith("/**") ? relative : `${relative}/**`;
}

function RuleTags({
  values,
  empty,
  onRemove
}: {
  values: string[];
  empty: string;
  onRemove: (value: string) => void;
}) {
  if (values.length === 0) {
    return <div className="text-xs text-[var(--text-muted)]">{empty}</div>;
  }
  return (
    <div className="flex flex-wrap gap-2">
      {values.map((value) => (
        <span
          key={value}
          className="inline-flex max-w-full items-center gap-1 rounded-full border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-2.5 py-1 text-xs text-[var(--text-secondary)]"
        >
          <span className="max-w-[240px] truncate font-mono" title={value}>{value}</span>
          <button
            type="button"
            onClick={() => onRemove(value)}
            className="text-[var(--text-muted)] transition hover:text-red-400"
            aria-label={`remove ${value}`}
          >
            <X className="h-3 w-3" />
          </button>
        </span>
      ))}
    </div>
  );
}

function ModeSwitch({
  value,
  onChange
}: {
  value: RuleMode;
  onChange: (next: RuleMode) => void;
}) {
  return (
    <div className="inline-flex rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] p-0.5">
      {(["include", "exclude"] as const).map((mode) => (
        <button
          key={mode}
          type="button"
          onClick={() => onChange(mode)}
          className={`rounded-md px-2.5 py-1 text-xs transition ${
            value === mode
              ? "bg-[var(--accent-soft)] text-[var(--accent)]"
              : "text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          }`}
        >
          {mode === "include" ? "白名单" : "黑名单"}
        </button>
      ))}
    </div>
  );
}

function ExtensionRuleBox({
  mode,
  values,
  onModeChange,
  onValuesChange
}: {
  mode: RuleMode;
  values: string[];
  onModeChange: (mode: RuleMode) => void;
  onValuesChange: (values: string[]) => void;
}) {
  const toggle = (ext: string) => {
    onValuesChange(values.includes(ext) ? removeValue(values, ext) : uniq([...values, ext]));
  };

  return (
    <div className="space-y-2 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm text-[var(--text-primary)]">文件类型</div>
          <div className="mt-1 text-xs text-[var(--text-secondary)]">
            {mode === "include" ? "白名单填了就只读这些类型；不填默认全读。" : "黑名单填了就不读这些类型；不填默认全读。"}
          </div>
        </div>
        <ModeSwitch value={mode} onChange={onModeChange} />
      </div>
      <div className="flex flex-wrap gap-1.5">
        {SUPPORTED_EXTENSIONS.map((ext) => {
          const active = values.includes(ext);
          return (
            <button
              key={ext}
              type="button"
              onClick={() => toggle(ext)}
              className={`rounded-md border px-2.5 py-1 font-mono text-xs transition ${
                active
                  ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                  : "border-[var(--border-subtle)] text-[var(--text-secondary)] hover:text-[var(--accent)]"
              }`}
            >
              .{ext}
            </button>
          );
        })}
      </div>
      <RuleTags values={values} empty="未选择文件类型" onRemove={(value) => onValuesChange(removeValue(values, value))} />
    </div>
  );
}

function PathRuleBox({
  mode,
  values,
  watchRoot,
  onModeChange,
  onValuesChange
}: {
  mode: RuleMode;
  values: string[];
  watchRoot?: string;
  onModeChange: (mode: RuleMode) => void;
  onValuesChange: (values: string[]) => void;
}) {
  const [items, setItems] = useState<SearchScopeItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(() => new Set());

  const loadItems = async () => {
    if (!watchRoot) return;
    setLoading(true);
    setError(null);
    try {
      const next = await listSearchScopes();
      setItems(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadItems();
  }, [watchRoot]);

  const viewItems = useMemo(() => {
    const normalized = items.map((item) => {
      const key = (item.relative_path || item.name || item.path).replace(/\\/g, "/");
      const slashIndex = key.lastIndexOf("/");
      return {
        ...item,
        key,
        parentKey: slashIndex >= 0 ? key.slice(0, slashIndex) : ""
      };
    });
    const byKey = new Map(normalized.map((item) => [item.key, item] as const));
    const isVisible = (item: (typeof normalized)[number]): boolean => {
      if (!item.parentKey) return true;
      const parent = byKey.get(item.parentKey);
      if (!parent || !parent.is_dir) return true;
      return isVisible(parent) && expandedDirs.has(parent.key);
    };
    return normalized.filter(isVisible);
  }, [expandedDirs, items]);

  const addItem = (item: SearchScopeItem) => {
    onValuesChange(uniq([...values, scopeItemToRule(item)]));
  };

  const toggleDir = (key: string) => {
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  return (
    <div className="space-y-2 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] px-3 py-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm text-[var(--text-primary)]">文件夹 / 文件</div>
          <div className="mt-1 text-xs text-[var(--text-secondary)]">
            {mode === "include" ? "白名单填了就只读这些文件夹/文件；不填默认全读。" : "黑名单填了就跳过这些文件夹/文件；不填默认全读。"}
          </div>
        </div>
        <ModeSwitch value={mode} onChange={onModeChange} />
      </div>
      <div className="max-h-56 overflow-y-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)]">
        <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-2 py-1.5 text-xs text-[var(--text-muted)]">
          <span>{watchRoot ? "点击下面的文件夹或文件加入规则" : "请先选择读取文件夹"}</span>
          <button
            type="button"
            onClick={() => void loadItems()}
            disabled={loading || !watchRoot}
            className="inline-flex items-center gap-1 text-[var(--text-secondary)] transition hover:text-[var(--accent)] disabled:opacity-50"
          >
            <RefreshCw className={`h-3 w-3 ${loading ? "animate-spin" : ""}`} />
            刷新
          </button>
        </div>
        {error ? <div className="px-2 py-2 text-xs text-red-400">{error}</div> : null}
        {!loading && !error && viewItems.length === 0 ? (
          <div className="px-2 py-2 text-xs text-[var(--text-muted)]">暂无可选文件或文件夹</div>
        ) : null}
        {viewItems.slice(0, 300).map((item) => {
          const key = item.key;
          const rule = scopeItemToRule(item);
          const selected = values.includes(rule);
          const expanded = expandedDirs.has(key);
          const Icon = item.is_dir ? Folder : FileText;
          return (
            <div
              key={item.path}
              className="flex items-center gap-1 border-b border-[var(--border-subtle)]/50 px-2 py-1.5 last:border-b-0"
              style={{ paddingLeft: `${8 + item.depth * 14}px` }}
            >
              {item.is_dir ? (
                <button
                  type="button"
                  onClick={() => toggleDir(key)}
                  className="text-[var(--text-muted)] transition hover:text-[var(--accent)]"
                  aria-label={expanded ? "折叠文件夹" : "展开文件夹"}
                >
                  {expanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                </button>
              ) : (
                <span className="h-3.5 w-3.5" />
              )}
              <button
                type="button"
                onClick={() => addItem(item)}
                className={`flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-xs transition ${
                  selected
                    ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
                }`}
                title={item.relative_path || item.path}
              >
                <Icon className="h-3.5 w-3.5 shrink-0" />
                <span className="truncate">{item.name}</span>
              </button>
            </div>
          );
        })}
      </div>
      <RuleTags values={values} empty="未选择文件夹或文件" onRemove={(value) => onValuesChange(removeValue(values, value))} />
    </div>
  );
}

function FilterBody({
  filterConfig,
  filterBusy,
  filterMessage,
  watchRoot,
  onFilterConfigChange,
  onSaveFilterConfig
}: Omit<FilterPanelProps, "initiallyOpen">) {
  const [extensionMode, setExtensionMode] = useState<RuleMode>(
    filterConfig.include_extensions.length > 0 ? "include" : "exclude"
  );
  const [pathMode, setPathMode] = useState<RuleMode>(
    filterConfig.include_paths.length > 0 ? "include" : "exclude"
  );

  const extensionValues =
    extensionMode === "include" ? filterConfig.include_extensions : filterConfig.exclude_extensions;
  const pathValues = pathMode === "include" ? filterConfig.include_paths : filterConfig.exclude_paths;

  const setExtensionValues = (values: string[]) => {
    const normalized = uniq(values.map(normalizeExt).filter((value) => SUPPORTED_EXTENSIONS.includes(value)));
    onFilterConfigChange({
      ...filterConfig,
      include_extensions: extensionMode === "include" ? normalized : [],
      exclude_extensions: extensionMode === "exclude" ? normalized : []
    });
  };

  const setPathValues = (values: string[]) => {
    const normalized = uniq(values);
    onFilterConfigChange({
      ...filterConfig,
      include_paths: pathMode === "include" ? normalized : [],
      exclude_paths: pathMode === "exclude" ? normalized : []
    });
  };

  const changeExtensionMode = (mode: RuleMode) => {
    setExtensionMode(mode);
    onFilterConfigChange({
      ...filterConfig,
      include_extensions: mode === "include" ? extensionValues : [],
      exclude_extensions: mode === "exclude" ? extensionValues : []
    });
  };

  const changePathMode = (mode: RuleMode) => {
    setPathMode(mode);
    onFilterConfigChange({
      ...filterConfig,
      include_paths: mode === "include" ? pathValues : [],
      exclude_paths: mode === "exclude" ? pathValues : []
    });
  };

  const clearAll = () => {
    onFilterConfigChange({
      enabled: false,
      include_extensions: [],
      exclude_extensions: [],
      include_paths: [],
      exclude_paths: [],
      min_mtime: null,
      max_mtime: null,
      min_size: null,
      max_size: null
    });
  };

  return (
    <div className="space-y-3">
      <ExtensionRuleBox
        mode={extensionMode}
        values={extensionValues}
        onModeChange={changeExtensionMode}
        onValuesChange={setExtensionValues}
      />
      <PathRuleBox
        mode={pathMode}
        values={pathValues}
        watchRoot={watchRoot}
        onModeChange={changePathMode}
        onValuesChange={setPathValues}
      />
      {filterMessage ? (
        <div className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--accent)]">
          {filterMessage}
        </div>
      ) : null}
      <div className="flex justify-end gap-2">
        <AnimatedPressButton
          type="button"
          onClick={clearAll}
          className="rounded-lg border border-[var(--border-subtle)] px-3 py-1.5 text-xs text-[var(--text-secondary)] transition hover:text-red-400"
        >
          清空
        </AnimatedPressButton>
      </div>
    </div>
  );
}

export function FilterPanel({
  filterConfig,
  filterBusy,
  filterMessage,
  watchRoot,
  initiallyOpen = false,
  onFilterConfigChange,
  onSaveFilterConfig
}: FilterPanelProps) {
  const [open, setOpen] = useState(initiallyOpen);
  const ruleCount =
    filterConfig.include_extensions.length +
    filterConfig.exclude_extensions.length +
    filterConfig.include_paths.length +
    filterConfig.exclude_paths.length;

  return (
    <AnimatedPanel className="glass-panel-infer space-y-3 rounded-lg px-3 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <button
            type="button"
            onClick={() => setOpen((prev) => !prev)}
            className="inline-flex items-center gap-1 text-sm text-[var(--text-primary)] transition hover:text-[var(--accent)]"
          >
            {open ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
            读取筛选
          </button>
          <div className="mt-1 truncate text-xs text-[var(--text-secondary)]">
            {filterConfig.enabled ? `已启用，当前 ${ruleCount} 条规则` : "未启用，默认全部读取"}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2" onClick={(event) => event.stopPropagation()}>
          <CyberToggle
            checked={filterConfig.enabled}
            onChange={(enabled) => onFilterConfigChange({ ...filterConfig, enabled })}
            ariaLabel="启用读取筛选"
          />
        </div>
      </div>

      {open ? (
        <FilterBody
          filterConfig={filterConfig}
          filterBusy={filterBusy}
          filterMessage={filterMessage}
          watchRoot={watchRoot}
          onFilterConfigChange={onFilterConfigChange}
          onSaveFilterConfig={onSaveFilterConfig}
        />
      ) : null}
    </AnimatedPanel>
  );
}

export function FilterTab(props: FilterPanelProps) {
  const { t } = useI18n();
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.18 }}
      className="space-y-4"
    >
      <h3 className="text-base font-semibold text-[var(--text-primary)]">{t("fileFilter")}</h3>
      <FilterPanel {...props} />
    </motion.div>
  );
}
