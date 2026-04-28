import { motion } from "framer-motion";
import { useI18n } from "../../../i18n";
import { AnimatedPressButton } from "../../MotionKit";
import { SettingCard, SelectionChips } from "../controls";
import type { IndexFilterConfigDto } from "../types";

type FilterTabProps = {
  filterConfig: IndexFilterConfigDto;
  filterBusy: boolean;
  filterMessage: string | null;
  onFilterConfigChange: (next: IndexFilterConfigDto) => void;
  onSaveFilterConfig: () => Promise<void>;
};

function linesToArray(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function arrayToLines(arr: string[]): string {
  return arr.join("\n");
}

function bytesToDisplay(bytes: number | null): string {
  if (bytes === null || bytes === undefined) return "";
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

export function FilterTab({
  filterConfig,
  filterBusy,
  filterMessage,
  onFilterConfigChange,
  onSaveFilterConfig,
}: FilterTabProps) {
  const { t } = useI18n();

  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.18 }}
      className="space-y-4"
    >
      <h3 className="text-base font-semibold text-[var(--text-primary)]">
        {t("fileFilter")}
      </h3>

      <SettingCard
        title={t("filterEnabled")}
        description={t("filterEnabledDesc")}
      >
        <SelectionChips
          value={filterConfig.enabled ? "enabled" : "disabled"}
          onChange={(value) =>
            onFilterConfigChange({ ...filterConfig, enabled: value === "enabled" })
          }
          options={[
            { value: "enabled", label: t("enabled") },
            { value: "disabled", label: t("disabled") },
          ]}
        />
      </SettingCard>

      <div className="space-y-3">
        <h4 className="text-sm font-medium text-[var(--text-primary)]">
          {t("extensions")}
        </h4>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="text-sm text-[var(--text-primary)]">
              {t("includeExtensions")}
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              {t("includeExtensionsDesc")}
            </div>
            <textarea
              value={arrayToLines(filterConfig.include_extensions)}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  include_extensions: linesToArray(e.target.value),
                })
              }
              placeholder="md&#10;txt"
              rows={4}
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </div>
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="text-sm text-[var(--text-primary)]">
              {t("excludeExtensions")}
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              {t("excludeExtensionsDesc")}
            </div>
            <textarea
              value={arrayToLines(filterConfig.exclude_extensions)}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  exclude_extensions: linesToArray(e.target.value),
                })
              }
              placeholder="tmp&#10;log"
              rows={4}
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </div>
        </div>
      </div>

      <div className="space-y-3">
        <h4 className="text-sm font-medium text-[var(--text-primary)]">
          {t("pathPatterns")}
        </h4>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="text-sm text-[var(--text-primary)]">
              {t("excludePaths")}
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              {t("excludePathsDesc")}
            </div>
            <textarea
              value={arrayToLines(filterConfig.exclude_paths)}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  exclude_paths: linesToArray(e.target.value),
                })
              }
              placeholder="drafts&#10;temp/**&#10;**/*.bak"
              rows={4}
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </div>
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="text-sm text-[var(--text-primary)]">
              {t("includePaths")}
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              {t("includePathsDesc")}
            </div>
            <textarea
              value={arrayToLines(filterConfig.include_paths)}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  include_paths: linesToArray(e.target.value),
                })
              }
              placeholder="important/**&#10;notes/keep.md"
              rows={4}
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </div>
        </div>
      </div>

      <div className="space-y-3">
        <h4 className="text-sm font-medium text-[var(--text-primary)]">
          {t("dateRange")}
        </h4>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <SettingCard title={t("minDate")} description={t("minDateDesc")}>
            <input
              type="date"
              value={filterConfig.min_mtime ?? ""}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  min_mtime: e.target.value || null,
                })
              }
              className="h-9 rounded-lg border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </SettingCard>
          <SettingCard title={t("maxDate")} description={t("maxDateDesc")}>
            <input
              type="date"
              value={filterConfig.max_mtime ?? ""}
              onChange={(e) =>
                onFilterConfigChange({
                  ...filterConfig,
                  max_mtime: e.target.value || null,
                })
              }
              className="h-9 rounded-lg border border-transparent bg-transparent px-2 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
          </SettingCard>
        </div>
      </div>

      <div className="space-y-3">
        <h4 className="text-sm font-medium text-[var(--text-primary)]">
          {t("sizeRange")}
        </h4>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="flex items-center justify-between">
              <div className="text-sm text-[var(--text-primary)]">{t("minSize")}</div>
              <div className="text-xs text-[var(--text-secondary)]">
                {bytesToDisplay(filterConfig.min_size)}
              </div>
            </div>
            <input
              type="number"
              min={0}
              value={filterConfig.min_size ?? ""}
              onChange={(e) => {
                const val = e.target.value === "" ? null : Number(e.target.value);
                onFilterConfigChange({ ...filterConfig, min_size: val });
              }}
              placeholder="0"
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
            <div className="text-xs text-[var(--text-secondary)]">{t("sizeInBytes")}</div>
          </div>
          <div className="glass-panel-infer space-y-2 rounded-lg px-3 py-3">
            <div className="flex items-center justify-between">
              <div className="text-sm text-[var(--text-primary)]">{t("maxSize")}</div>
              <div className="text-xs text-[var(--text-secondary)]">
                {bytesToDisplay(filterConfig.max_size)}
              </div>
            </div>
            <input
              type="number"
              min={0}
              value={filterConfig.max_size ?? ""}
              onChange={(e) => {
                const val = e.target.value === "" ? null : Number(e.target.value);
                onFilterConfigChange({ ...filterConfig, max_size: val });
              }}
              placeholder="∞"
              className="w-full rounded-lg border border-transparent bg-transparent px-2 py-1.5 text-sm text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
            />
            <div className="text-xs text-[var(--text-secondary)]">{t("sizeInBytes")}</div>
          </div>
        </div>
      </div>

      {filterMessage ? (
        <div className="text-sm text-[var(--accent)]">{filterMessage}</div>
      ) : null}

      <div className="pt-2">
        <AnimatedPressButton
          onClick={() => void onSaveFilterConfig()}
          disabled={filterBusy}
          className="w-full rounded-lg px-4 py-2 text-sm font-medium text-[var(--accent)] transition hover:bg-[var(--accent-soft)] disabled:opacity-45"
        >
          {filterBusy ? t("saving") : t("saveFilterSettings")}
        </AnimatedPressButton>
      </div>
    </motion.div>
  );
}
