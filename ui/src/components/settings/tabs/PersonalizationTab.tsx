import { motion } from "framer-motion";
import { fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { SelectionChips, SettingCard } from "../controls";
import type { FontPreset, FontScale, ThemeMode } from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type PersonalizationTabProps = {
  t: TranslateFn;
  fontPreset: FontPreset;
  onFontPresetChange: (preset: FontPreset) => void;
  fontScale: FontScale;
  onFontScaleChange: (scale: FontScale) => void;
  themeMode: ThemeMode;
  onThemeModeChange: (mode: ThemeMode) => void;
  fontPresetOptions: Array<{ value: FontPreset; label: string }>;
  fontScaleOptions: Array<{ value: FontScale; label: string }>;
};

export function PersonalizationTab({
  t,
  fontPreset,
  onFontPresetChange,
  fontScale,
  onFontScaleChange,
  themeMode,
  onThemeModeChange,
  fontPresetOptions,
  fontScaleOptions
}: PersonalizationTabProps) {
  return (
    <motion.div
      key="settings-tab-personalization"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("personalization")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4">
        <SettingCard title={t("fontPreset")}>
          <SelectionChips value={fontPreset} onChange={onFontPresetChange} options={fontPresetOptions} />
        </SettingCard>
        <SettingCard title={t("fontSize")}>
          <SelectionChips value={fontScale} onChange={onFontScaleChange} options={fontScaleOptions} />
        </SettingCard>
        <SettingCard title={t("themeToggle")} description={t("themeToggleDesc")}>
          <SelectionChips
            value={themeMode}
            onChange={onThemeModeChange}
            options={[
              { value: "dark", label: t("themeModeDark") },
              { value: "light", label: t("themeModeLight") }
            ]}
          />
        </SettingCard>
      </motion.div>
    </motion.div>
  );
}
