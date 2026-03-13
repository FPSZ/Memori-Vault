import { AnimatePresence, motion } from "framer-motion";
import { Check, ChevronDown, ChevronRight, FileText, FolderOpen, LoaderCircle, Search } from "lucide-react";
import type { RefObject } from "react";
import { useI18n } from "../../i18n";
import type { FileMatch, SearchScopeItem } from "../types";
import type { Language } from "../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type SearchStageProps = {
  isSearching: boolean;
  isSearchBarCollapsed: boolean;
  isSearchBarCompact: boolean;
  allowCompactHoverExpand: boolean;
  isSearchInputFocused: boolean;
  scopeMenuOpen: boolean;
  scopeLoading: boolean;
  scopeItems: SearchScopeItem[];
  visibleScopeItems: Array<SearchScopeItem & { key: string }>;
  selectedScopeSet: Set<string>;
  selectedScopeLabel: string;
  scopeChildrenCountByParentKey: Map<string, number>;
  expandedScopeDirs: Set<string>;
  fileMatchesOpen: boolean;
  fileMatches: FileMatch[];
  watchRoot: string;
  showSearchDone: boolean;
  loading: boolean;
  modelSetupNotConfigured: boolean;
  query: string;
  uiLang: Language;
  searchPlaceholder: string;
  t: TranslateFn;
  searchInputRef: RefObject<HTMLInputElement | null>;
  scopeMenuRef: RefObject<HTMLDivElement | null>;
  fileMatchesCloseTimerRef: RefObject<number | null>;
  setQuery: (value: string) => void;
  setIsSearchBarHovering: (value: boolean) => void;
  setScopeMenuOpen: (next: boolean | ((prev: boolean) => boolean)) => void;
  setIsSearchInputFocused: (value: boolean) => void;
  setFileMatchesOpen: (value: boolean) => void;
  setSelectedScopePaths: (next: string[] | ((prev: string[]) => string[])) => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onClearScopeSelection: () => void;
  onToggleScopePath: (path: string) => void;
  onToggleScopeDirExpanded: (path: string) => void;
};

export function SearchStage({
  isSearching,
  isSearchBarCollapsed,
  isSearchBarCompact,
  allowCompactHoverExpand,
  isSearchInputFocused,
  scopeMenuOpen,
  scopeLoading,
  scopeItems,
  visibleScopeItems,
  selectedScopeSet,
  selectedScopeLabel,
  scopeChildrenCountByParentKey,
  expandedScopeDirs,
  fileMatchesOpen,
  fileMatches,
  watchRoot,
  showSearchDone,
  loading,
  modelSetupNotConfigured,
  query,
  uiLang,
  searchPlaceholder,
  t,
  searchInputRef,
  scopeMenuRef,
  fileMatchesCloseTimerRef,
  setQuery,
  setIsSearchBarHovering,
  setScopeMenuOpen,
  setIsSearchInputFocused,
  setFileMatchesOpen,
  setSelectedScopePaths,
  onKeyDown,
  onClearScopeSelection,
  onToggleScopePath,
  onToggleScopeDirExpanded
}: SearchStageProps) {
  return (
    <motion.div
      className="absolute left-6 right-6 z-40 md:left-10 md:right-10"
      animate={{
        top: isSearching ? (isSearchBarCollapsed ? "6px" : isSearchBarCompact ? "8px" : "20px") : "45%",
        y: isSearching ? 0 : "-50%"
      }}
      transition={{ duration: 0.18, ease: "easeOut" }}
    >
      <motion.div
        onMouseEnter={() => {
          if (isSearchBarCompact && allowCompactHoverExpand) {
            setIsSearchBarHovering(true);
          }
        }}
        onMouseLeave={() => {
          if (isSearchBarCompact && !isSearchInputFocused && !scopeMenuOpen) {
            setIsSearchBarHovering(false);
          }
        }}
        className={`relative mx-auto w-full transition-[max-width,opacity,box-shadow,background-color,border-color] duration-300 ease-out will-change-[max-width,opacity] focus-within:ring-1 focus-within:ring-[var(--line-soft)] focus-within:shadow-[var(--float-shadow-focus)] ${
          isSearchBarCollapsed
            ? "max-w-[300px] rounded-full overflow-hidden border-0 bg-transparent px-0 py-0 shadow-none ring-0 focus-within:ring-0"
            : isSearching && isSearchBarCompact
              ? "max-w-3xl rounded-full px-4 py-2.5"
              : "max-w-4xl rounded-xl px-6 py-5"
        } ${
          isSearchBarCollapsed
            ? ""
            : isSearching && isSearchBarCompact
              ? "surface-lite ring-0 shadow-[0_2px_10px_rgba(15,23,42,0.08)]"
              : isSearching
                ? "surface-lite ring-0 shadow-[var(--float-shadow)]"
                : "surface-lite ring-0 shadow-[var(--float-shadow)]"
        }`}
      >
        {isSearchBarCollapsed ? (
          <button
            type="button"
            onClick={() => {
              setIsSearchBarHovering(true);
              requestAnimationFrame(() => searchInputRef.current?.focus());
            }}
            aria-label={searchPlaceholder}
            className="block h-1.5 w-full appearance-none rounded-full border-0 bg-[var(--search-collapsed-bar)] p-0 shadow-[0_2px_8px_rgba(15,23,42,0.12)] outline-none focus:border-0 focus:outline-none focus-visible:outline-none focus:ring-0 focus-visible:ring-0"
          />
        ) : (
          <>
            <div className="relative flex items-center gap-3">
              <div ref={scopeMenuRef} className="relative shrink-0">
                <button
                  type="button"
                  onClick={() => setScopeMenuOpen((prev) => !prev)}
                  className={`inline-flex max-w-[170px] items-center gap-1.5 rounded-lg border border-transparent px-2.5 text-xs transition ${
                    isSearching && isSearchBarCompact ? "h-8" : "h-9"
                  } ${
                    scopeMenuOpen
                      ? "bg-[var(--accent-soft)] text-[var(--accent)]"
                      : "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
                  }`}
                  aria-label={t("scopeSelectTitle")}
                  title={t("scopeSelectTitle")}
                >
                  <ChevronDown
                    className={`h-3.5 w-3.5 shrink-0 transition-transform ${scopeMenuOpen ? "rotate-180" : ""}`}
                  />
                  <span className="truncate">{selectedScopeLabel}</span>
                </button>

                <AnimatePresence>
                  {scopeMenuOpen && (
                    <motion.div
                      initial={{ opacity: 0, y: -6, scale: 0.98 }}
                      animate={{ opacity: 1, y: 0, scale: 1 }}
                      exit={{ opacity: 0, y: -4, scale: 0.98 }}
                      transition={{ duration: 0.16, ease: "easeOut" }}
                      className="surface-lite-strong absolute left-0 top-[calc(100%+10px)] z-40 w-[360px] rounded-xl p-2 shadow-[0_20px_45px_rgba(0,0,0,0.5)]"
                    >
                      <div className="mb-2 flex items-center justify-between px-1">
                        <span className="text-[11px] tracking-[0.08em] text-[var(--text-secondary)]">
                          {t("scopeSelectTitle")}
                        </span>
                        <button
                          type="button"
                          onClick={onClearScopeSelection}
                          className="text-[11px] text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                        >
                          {t("scopeAll")}
                        </button>
                      </div>

                      <div className="no-scrollbar max-h-72 overflow-y-auto pr-1">
                        {scopeLoading && (
                          <div className="px-2 py-3 text-xs text-[var(--text-secondary)]">{t("scopeLoading")}</div>
                        )}

                        {!scopeLoading && scopeItems.length === 0 && (
                          <div className="px-2 py-3 text-xs text-[var(--text-secondary)]">{t("scopeNoItems")}</div>
                        )}

                        {!scopeLoading &&
                          visibleScopeItems.map((item) => {
                            const selected = selectedScopeSet.has(item.path);
                            const displayName = item.name.trim() ? item.name : item.path;
                            const relativePath = item.relative_path.trim() || item.path;
                            const hasChildren =
                              item.is_dir && (scopeChildrenCountByParentKey.get(item.key) ?? 0) > 0;
                            const isExpanded = expandedScopeDirs.has(item.path);

                            return (
                              <div
                                key={item.key}
                                onClick={() => onToggleScopePath(item.path)}
                                className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-left transition ${
                                  selected ? "bg-[var(--accent-soft)]" : "hover:bg-[var(--bg-surface-2)]"
                                }`}
                                title={item.path}
                                role="button"
                                tabIndex={0}
                                onKeyDown={(event) => {
                                  if (event.key === "Enter" || event.key === " ") {
                                    event.preventDefault();
                                    onToggleScopePath(item.path);
                                  }
                                }}
                              >
                                <span
                                  className="flex min-w-0 items-center gap-2"
                                  style={{ paddingLeft: `${item.depth * 12}px` }}
                                >
                                  {item.is_dir ? (
                                    <FolderOpen className="h-3.5 w-3.5 shrink-0 text-[var(--accent)]" />
                                  ) : (
                                    <FileText className="h-3.5 w-3.5 shrink-0 text-[var(--text-secondary)]" />
                                  )}
                                  <span className="min-w-0">
                                    <span className="block truncate text-xs text-[var(--text-primary)]">
                                      {displayName}
                                    </span>
                                    <span className="block truncate text-[10px] text-[var(--text-muted)]">
                                      {relativePath}
                                    </span>
                                  </span>
                                </span>
                                <span className="ml-2 inline-flex shrink-0 items-center gap-1">
                                  <span className="h-4 w-4">{selected ? <Check className="h-4 w-4 text-[var(--accent)]" /> : null}</span>
                                  {hasChildren ? (
                                    <button
                                      type="button"
                                      onClick={(event) => {
                                        event.stopPropagation();
                                        onToggleScopeDirExpanded(item.path);
                                      }}
                                      className="inline-flex h-4 w-4 items-center justify-center text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                                      aria-label={isExpanded ? "Collapse folder" : "Expand folder"}
                                      title={isExpanded ? "Collapse folder" : "Expand folder"}
                                    >
                                      <ChevronRight
                                        className={`h-3.5 w-3.5 transition-transform ${
                                          isExpanded ? "rotate-90 text-[var(--accent)]" : ""
                                        }`}
                                      />
                                    </button>
                                  ) : (
                                    <span className="h-4 w-4" />
                                  )}
                                </span>
                              </div>
                            );
                          })}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>

              <Search className="h-5 w-5 shrink-0 text-[var(--text-secondary)]" />
              <input
                ref={searchInputRef}
                type="text"
                autoFocus
                disabled={modelSetupNotConfigured}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={onKeyDown}
                onFocus={() => {
                  setIsSearchInputFocused(true);
                  if (isSearchBarCompact) {
                    setIsSearchBarHovering(true);
                  }
                  if (fileMatchesCloseTimerRef.current != null) {
                    window.clearTimeout(fileMatchesCloseTimerRef.current);
                    fileMatchesCloseTimerRef.current = null;
                  }
                }}
                onBlur={() => {
                  setIsSearchInputFocused(false);
                  if (isSearchBarCompact && !scopeMenuOpen) {
                    setIsSearchBarHovering(false);
                  }
                  fileMatchesCloseTimerRef.current = window.setTimeout(() => {
                    setFileMatchesOpen(false);
                  }, 120);
                }}
                placeholder={searchPlaceholder}
                className={`w-full flex-1 border-none bg-transparent pr-10 text-xl text-[var(--text-primary)] focus:outline-none focus:ring-0 disabled:cursor-not-allowed disabled:opacity-100 ${
                  modelSetupNotConfigured ? "placeholder:text-red-400" : "placeholder:text-[var(--text-muted)]"
                }`}
              />
            </div>
            <AnimatePresence>
              {fileMatchesOpen && fileMatches.length > 0 && (
                <motion.div
                  initial={{ opacity: 0, y: -6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -6 }}
                  transition={{ duration: 0.18, ease: "easeOut" }}
                  onMouseDown={(event) => event.preventDefault()}
                  className="surface-lite absolute left-0 right-0 top-full z-50 mt-2 overflow-hidden rounded-xl ring-1 ring-[var(--line-soft)] shadow-[var(--float-shadow)]"
                >
                  <div className="px-3 py-2 text-[11px] text-[var(--text-muted)]">
                    {uiLang === "zh-CN" ? "相关文件" : "Relevant files"}
                  </div>
                  <div className="settings-scrollbar max-h-72 overflow-y-auto pr-1">
                    {fileMatches.slice(0, 20).map((item) => {
                      const isSelected = selectedScopeSet.has(item.file_path);
                      const parent = item.parent_dir || "";
                      const relative =
                        watchRoot && parent.toLowerCase().startsWith(watchRoot.toLowerCase())
                          ? parent.slice(watchRoot.length).replace(/^[/\\\\]/, "")
                          : parent;
                      return (
                        <button
                          key={item.file_path}
                          type="button"
                          aria-pressed={isSelected}
                          onClick={() => {
                            setSelectedScopePaths((prev) => {
                              if (prev.includes(item.file_path)) {
                                return prev.filter((p) => p !== item.file_path);
                              }
                              return [...prev, item.file_path];
                            });
                            requestAnimationFrame(() => searchInputRef.current?.focus());
                          }}
                          className={`group flex w-full items-center gap-3 px-3 py-2 text-left transition-[background-color,transform] duration-180 ease-out active:scale-[0.995] ${
                            isSelected
                              ? "bg-[color-mix(in_srgb,var(--accent)_12%,transparent)]"
                              : "hover:bg-[color-mix(in_srgb,var(--accent)_6%,transparent)]"
                          }`}
                        >
                          <FileText
                            className={`h-4 w-4 shrink-0 ${
                              isSelected ? "text-[var(--accent)]" : "text-[var(--text-secondary)]"
                            }`}
                          />
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-sm text-[var(--text-primary)]">
                              {item.file_name || item.file_path}
                            </span>
                            <span className="block truncate text-[11px] text-[var(--text-muted)]">
                              {relative || item.parent_dir}
                            </span>
                          </span>
                          <span
                            className={`ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded-full transition-colors duration-180 ease-out ${
                              isSelected
                                ? "bg-[color-mix(in_srgb,var(--accent)_18%,transparent)] text-[var(--accent)]"
                                : "bg-[color-mix(in_srgb,var(--text-muted)_6%,transparent)] text-[color-mix(in_srgb,var(--text-muted)_20%,transparent)] group-hover:bg-[color-mix(in_srgb,var(--accent)_8%,transparent)]"
                            }`}
                          >
                            <Check
                              className={`h-3.5 w-3.5 transition-[opacity,transform] duration-180 ease-out ${
                                isSelected ? "opacity-100 scale-100" : "opacity-0 scale-75"
                              }`}
                            />
                          </span>
                        </button>
                      );
                    })}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
            <AnimatePresence>
              {isSearching && loading && (
                <motion.div
                  initial={{ opacity: 0.2, scale: 0.95 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ repeat: Infinity, repeatType: "reverse", duration: 0.9 }}
                  className="absolute right-6 top-1/2 -translate-y-1/2 text-[var(--accent)]"
                >
                  <LoaderCircle className="h-5 w-5 animate-spin" />
                </motion.div>
              )}
              {showSearchDone && (
                <motion.div
                  initial={{ opacity: 0, scale: 0.92 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.2 }}
                  className="absolute right-6 top-1/2 -translate-y-1/2 text-[var(--accent)]"
                >
                  <Check className="h-5 w-5" />
                </motion.div>
              )}
            </AnimatePresence>
          </>
        )}
      </motion.div>
    </motion.div>
  );
}
