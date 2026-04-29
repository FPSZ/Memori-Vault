import { motion } from "framer-motion";
import { AnimatedPanel, AnimatedPressButton, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { SelectionChips, SettingCard } from "../controls";
import type { MemorySettingsDto } from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type MemoryTabProps = {
  t: TranslateFn;
  memorySettings: MemorySettingsDto;
  memoryBusy: boolean;
  memoryMessage: string | null;
  onMemorySettingsChange: (next: MemorySettingsDto) => void;
};

export function MemoryTab({
  t,
  memorySettings,
  memoryMessage,
  onMemorySettingsChange
}: MemoryTabProps) {
  return (
    <motion.div
      key="settings-tab-memory"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("memory")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4">
        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          <div className="text-sm text-[var(--text-primary)]">{t("memoryArchitectureTitle")}</div>
          <div className="mt-2 grid grid-cols-5 gap-2 text-center text-xs text-[var(--text-secondary)]">
            {["STM", "MTM", "LTM", "TKG", "Policy"].map((label) => (
              <span key={label} className="rounded-md border border-[var(--line-soft)] px-2 py-1">
                {label}
              </span>
            ))}
          </div>
          <div className="mt-3 text-xs leading-5 text-[var(--text-secondary)]">
            {t("memoryArchitectureDesc")}
          </div>
        </AnimatedPanel>

        <SettingCard title={t("conversationMemory")} description={t("conversationMemoryDesc")}>
          <SelectionChips
            value={memorySettings.conversation_memory_enabled ? "enabled" : "disabled"}
            onChange={(value) =>
              onMemorySettingsChange({
                ...memorySettings,
                conversation_memory_enabled: value === "enabled"
              })
            }
            options={[
              { value: "enabled", label: t("mcpEnabled") },
              { value: "disabled", label: t("mcpDisabled") }
            ]}
          />
        </SettingCard>

        <SettingCard title={t("autoMemoryWrite")} description={t("autoMemoryWriteDesc")}>
          <SelectionChips
            value={memorySettings.auto_memory_write}
            onChange={(value) =>
              onMemorySettingsChange({
                ...memorySettings,
                auto_memory_write: value
              })
            }
            options={[
              { value: "suggest", label: t("autoMemorySuggest") },
              { value: "off", label: t("autoMemoryOff") },
              { value: "auto_low_risk", label: t("autoMemoryLowRisk") }
            ]}
          />
        </SettingCard>

        <SettingCard title={t("memoryWriteSource")} description={t("memoryWriteSourceDesc")}>
          <SelectionChips
            value={memorySettings.memory_write_requires_source ? "required" : "optional"}
            onChange={(value) =>
              onMemorySettingsChange({
                ...memorySettings,
                memory_write_requires_source: value === "required"
              })
            }
            options={[
              { value: "required", label: t("memorySourceRequired") },
              { value: "optional", label: t("memorySourceOptional") }
            ]}
          />
        </SettingCard>

        <SettingCard title={t("memoryMarkdownExport")} description={t("memoryMarkdownExportDesc")}>
          <SelectionChips
            value="disabled"
            onChange={() =>
              onMemorySettingsChange({
                ...memorySettings,
                memory_markdown_export_enabled: false
              })
            }
            options={[
              { value: "disabled", label: t("mcpDisabled") },
              { value: "enabled", label: t("plannedCapability"), disabled: true }
            ]}
          />
        </SettingCard>

        <AnimatedPanel className="glass-panel-infer space-y-3 rounded-lg px-3 py-3">
          <div>
            <div className="text-sm text-[var(--text-primary)]">{t("contextBudget")}</div>
            <div className="mt-1 text-xs text-[var(--text-secondary)]">{t("contextBudgetDesc")}</div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
              <div className="mb-2 text-sm text-[var(--text-primary)]">{t("defaultContextBudget")}</div>
              <SelectionChips
                value={memorySettings.default_context_budget}
                onChange={(value) =>
                  onMemorySettingsChange({
                    ...memorySettings,
                    default_context_budget: value
                  })
                }
                options={[
                  { value: "8k", label: "8K" },
                  { value: "16k", label: "16K" },
                  { value: "32k", label: "32K" }
                ]}
              />
            </AnimatedPanel>
            <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
              <div className="mb-2 text-sm text-[var(--text-primary)]">{t("complexContextBudget")}</div>
              <SelectionChips
                value={memorySettings.complex_context_budget}
                onChange={(value) =>
                  onMemorySettingsChange({
                    ...memorySettings,
                    complex_context_budget: value
                  })
                }
                options={[
                  { value: "16k", label: "16K" },
                  { value: "32k", label: "32K" },
                  { value: "64k", label: "64K" }
                ]}
              />
            </AnimatedPanel>
          </div>
        </AnimatedPanel>

        <SettingCard title={t("graphRanking")} description={t("graphRankingDesc")}>
          <SelectionChips
            value="disabled"
            onChange={() =>
              onMemorySettingsChange({
                ...memorySettings,
                graph_ranking_enabled: false
              })
            }
            options={[
              { value: "disabled", label: t("graphRankingOff") },
              { value: "enabled", label: t("graphRankingPlanned"), disabled: true }
            ]}
          />
        </SettingCard>

        <AnimatedPanel className="rounded-lg border border-amber-400/20 bg-amber-500/10 px-3 py-3 text-xs text-amber-100">
          {t("memoryEvidenceFirewall")}
        </AnimatedPanel>

        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          {memoryMessage ? <span className="text-xs text-[var(--text-secondary)]">{memoryMessage}</span> : null}
        </AnimatedPanel>
      </motion.div>
    </motion.div>
  );
}
