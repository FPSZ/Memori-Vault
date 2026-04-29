import { motion } from "framer-motion";
import { Copy } from "lucide-react";
import { AnimatedPanel, AnimatedPressButton, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { CyberInput } from "../../UI";
import { SelectionChips, SettingCard } from "../controls";
import type { McpSettingsDto, McpStatusDto, McpTransportMode } from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type McpTabProps = {
  t: TranslateFn;
  mcpSettings: McpSettingsDto;
  mcpStatus: McpStatusDto | null;
  mcpBusy: boolean;
  mcpMessage: string | null;
  onMcpSettingsChange: (next: McpSettingsDto) => void;
  onCopyMcpClientConfig: (client: string) => Promise<void>;
};

export function McpTab({
  t,
  mcpSettings,
  mcpStatus,
  mcpBusy,
  mcpMessage,
  onMcpSettingsChange,
  onCopyMcpClientConfig
}: McpTabProps) {
  const transportMode: McpTransportMode = mcpSettings.transports.includes("stdio") && mcpSettings.transports.includes("http")
    ? "both"
    : mcpSettings.transports.includes("http")
      ? "http"
      : "stdio";

  const updateTransportMode = (mode: McpTransportMode) => {
    const transports: Array<"stdio" | "http"> = mode === "both" ? ["http", "stdio"] : [mode];
    onMcpSettingsChange({ ...mcpSettings, transports });
  };

  return (
    <motion.div
      key="settings-tab-mcp"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("mcp")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-4">
        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm text-[var(--text-primary)]">{t("mcpStatusTitle")}</div>
              <div className="mt-1 text-xs text-[var(--text-secondary)]">
                {mcpSettings.enabled ? t("mcpStatusEnabled") : t("mcpStatusDisabled")}
              </div>
            </div>
            <span className="rounded-full border border-[var(--line-soft)] px-3 py-1 text-xs text-[var(--accent)]">
              MCP {mcpStatus?.protocol_version ?? "2025-11-25"}
            </span>
          </div>
          <div className="mt-3 grid grid-cols-3 gap-2 text-xs text-[var(--text-secondary)]">
            <div>{t("mcpToolsCount", { count: mcpStatus?.tools_count ?? 25 })}</div>
            <div>{t("mcpResourcesCount", { count: mcpStatus?.resources_count ?? 5 })}</div>
            <div>{t("mcpPromptsCount", { count: mcpStatus?.prompts_count ?? 5 })}</div>
          </div>
        </AnimatedPanel>

        <SettingCard title={t("mcpEnable")} description={t("mcpEnableDesc")}>
          <SelectionChips
            value={mcpSettings.enabled ? "enabled" : "disabled"}
            onChange={(value) => onMcpSettingsChange({ ...mcpSettings, enabled: value === "enabled" })}
            options={[
              { value: "enabled", label: t("mcpEnabled") },
              { value: "disabled", label: t("mcpDisabled") }
            ]}
          />
        </SettingCard>

        <SettingCard title={t("mcpTransport")} description={t("mcpTransportDesc")}>
          <SelectionChips
            value={transportMode}
            onChange={updateTransportMode}
            options={[
              { value: "both", label: t("mcpTransportBoth") },
              { value: "stdio", label: t("mcpTransportStdio") },
              { value: "http", label: t("mcpTransportHttp") }
            ]}
          />
        </SettingCard>

        <AnimatedPanel className="glass-panel-infer space-y-3 rounded-lg px-3 py-3">
          <div>
            <div className="text-sm text-[var(--text-primary)]">{t("mcpEndpoint")}</div>
            <div className="mt-1 text-xs text-[var(--text-secondary)]">{t("mcpEndpointDesc")}</div>
          </div>
          <div className="grid grid-cols-[1fr_96px] gap-3">
            <CyberInput
              value={mcpSettings.http_bind}
              onChange={(value) => onMcpSettingsChange({ ...mcpSettings, http_bind: value })}
              placeholder="127.0.0.1"
            />
            <CyberInput
              value={String(mcpSettings.http_port)}
              onChange={(value) => onMcpSettingsChange({ ...mcpSettings, http_port: Number(value) || 3757 })}
              placeholder="3757"
            />
          </div>
          <code className="block rounded-md bg-[var(--accent-soft)] px-3 py-2 text-xs text-[var(--text-primary)]">
            {mcpStatus?.http_endpoint ?? `http://${mcpSettings.http_bind}:${mcpSettings.http_port}/mcp`}
          </code>
          <code className="block rounded-md bg-[var(--accent-soft)] px-3 py-2 text-xs text-[var(--text-primary)]">
            {mcpStatus?.stdio_command ?? "memori-server --mcp-stdio"}
          </code>
        </AnimatedPanel>

        <SettingCard title={t("mcpAccessMode")} description={t("mcpAccessModeDesc")}>
          <SelectionChips
            value={mcpSettings.access_mode}
            onChange={(value) => onMcpSettingsChange({ ...mcpSettings, access_mode: value })}
            options={[
              { value: "full_control", label: t("mcpAccessFull") },
              { value: "read_only", label: t("mcpAccessReadOnly") }
            ]}
          />
        </SettingCard>

        <AnimatedPanel className="rounded-lg border border-amber-400/20 bg-amber-500/10 px-3 py-3 text-xs text-amber-100">
          {t("mcpRiskWarning")}
        </AnimatedPanel>

        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          <div className="mb-3 text-sm text-[var(--text-primary)]">{t("mcpClientConfig")}</div>
          <div className="grid grid-cols-3 gap-2">
            {["claude", "codex", "opencode"].map((client) => (
              <AnimatedPressButton
                key={client}
                type="button"
                onClick={() => void onCopyMcpClientConfig(client)}
                className="flex items-center justify-center gap-2 rounded-md bg-transparent px-3 py-2 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
              >
                <Copy className="h-3.5 w-3.5" />
                {client}
              </AnimatedPressButton>
            ))}
          </div>
        </AnimatedPanel>

        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          <div className="mb-3 grid grid-cols-3 gap-2 text-xs text-[var(--text-secondary)]">
            <div>ask / search / get_source</div>
            <div>resources / templates</div>
            <div>prompts / graph / settings</div>
          </div>
          {mcpMessage ? <span className="text-xs text-[var(--text-secondary)]">{mcpMessage}</span> : null}
        </AnimatedPanel>
      </motion.div>
    </motion.div>
  );
}
