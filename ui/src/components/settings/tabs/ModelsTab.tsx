import { Fragment } from "react";
import { motion } from "framer-motion";
import { LoaderCircle } from "lucide-react";
import { AnimatedPanel, AnimatedPressButton, fadeSlideUpVariants, staggerContainerVariants } from "../../MotionKit";
import { CyberInput } from "../../UI";
import { ModelRoleSelector, SelectionChips, SettingCard } from "../controls";
import type {
  EnterprisePolicyDto,
  LocalModelProfileDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelRole,
  ModelSettingsDto,
  ProviderModelsDto,
  RemoteModelProfileDto
} from "../types";
import { useI18n } from "../../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];
export type ModelActionKey = "probe" | "refresh" | "save" | "pull";
type ActionPhase = "idle" | "running" | "success" | "error";

type ModelsTabProps = {
  t: TranslateFn;
  modelSettings: ModelSettingsDto;
  enterprisePolicy: EnterprisePolicyDto;
  modelAvailability: ModelAvailabilityDto | null;
  providerModels: ProviderModelsDto;
  modelBusy: boolean;
  enterpriseBusy: boolean;
  customMode: Record<ModelRole, boolean>;
  setCustomMode: (next: Record<ModelRole, boolean> | ((prev: Record<ModelRole, boolean>) => Record<ModelRole, boolean>)) => void;
  activeProviderPolicyBlock: string | null;
  onProviderSwitch: (provider: ModelProvider) => void;
  onEnterprisePolicyChange: (next: EnterprisePolicyDto) => void;
  onSaveEnterprisePolicy: () => Promise<void>;
  updateActiveProfile: (next: Partial<LocalModelProfileDto & RemoteModelProfileDto>) => void;
  onModelAction: (key: ModelActionKey, action: () => Promise<void>) => Promise<void>;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
  onSaveModelSettings: () => Promise<void>;
  onPullModel: (model: string) => Promise<void>;
  onPickLocalModelsRoot: () => Promise<void>;
  onClearLocalModelsRoot: () => void;
  actionState: Record<ModelActionKey, { phase: ActionPhase; tick: number }>;
  buttonClassByState: (key: ModelActionKey) => string;
  buttonLabelByState: (key: ModelActionKey) => string;
  stableActionButtonClass: string;
};

export function ModelsTab({
  t,
  modelSettings,
  enterprisePolicy,
  modelAvailability,
  providerModels,
  modelBusy,
  enterpriseBusy,
  customMode,
  setCustomMode,
  activeProviderPolicyBlock,
  onProviderSwitch,
  onEnterprisePolicyChange,
  onSaveEnterprisePolicy,
  updateActiveProfile,
  onModelAction,
  onProbeModelProvider,
  onRefreshProviderModels,
  onSaveModelSettings,
  onPullModel,
  onPickLocalModelsRoot,
  onClearLocalModelsRoot,
  actionState,
  buttonClassByState,
  buttonLabelByState,
  stableActionButtonClass
}: ModelsTabProps) {
  const activeProvider = modelSettings.active_provider;
  const activeProfile =
    activeProvider === "ollama_local" ? modelSettings.local_profile : modelSettings.remote_profile;

  return (
    <motion.div
      key="settings-tab-models"
      variants={fadeSlideUpVariants}
      initial="hidden"
      animate="show"
      exit="exit"
      className="pt-2"
    >
      <h3 className="mb-5 text-sm tracking-[0.16em] text-[var(--accent)] uppercase">{t("models")}</h3>
      <motion.div variants={staggerContainerVariants} initial="hidden" animate="show" className="space-y-3">
        <SettingCard title={t("modelProvider")}>
          <SelectionChips
            value={modelSettings.active_provider}
            onChange={onProviderSwitch}
            options={[
              { value: "ollama_local", label: t("providerOllama") },
              { value: "openai_compatible", label: t("providerOpenAI") }
            ]}
          />
        </SettingCard>

        <AnimatedPanel className="glass-panel-infer space-y-3 rounded-lg px-3 py-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm text-[var(--text-primary)]">{t("policyTitle")}</div>
              <div className="mt-1 text-xs text-[var(--text-secondary)]">{t("policyDesc")}</div>
            </div>
            <AnimatedPressButton
              type="button"
              onClick={() => void onSaveEnterprisePolicy()}
              disabled={enterpriseBusy}
              className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
            >
              {t("policySave")}
            </AnimatedPressButton>
          </div>
          <SelectionChips
            value={enterprisePolicy.egress_mode}
            onChange={(value) =>
              onEnterprisePolicyChange({
                ...enterprisePolicy,
                egress_mode: value
              })
            }
            options={[
              { value: "local_only", label: t("policyModeLocalOnly") },
              { value: "allowlist", label: t("policyModeAllowlist") }
            ]}
          />
          <div className="grid gap-3 md:grid-cols-2">
            <div>
              <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("policyAllowedEndpoints")}</div>
              <textarea
                value={enterprisePolicy.allowed_model_endpoints.join("\n")}
                onChange={(event) =>
                  onEnterprisePolicyChange({
                    ...enterprisePolicy,
                    allowed_model_endpoints: event.target.value
                      .split(/\r?\n/)
                      .map((item) => item.trim())
                      .filter(Boolean)
                  })
                }
                className="min-h-[96px] w-full rounded-lg border border-transparent bg-transparent px-3 py-2 text-xs text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                placeholder="https://models.company.local/v1"
              />
            </div>
            <div>
              <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("policyAllowedModels")}</div>
              <textarea
                value={enterprisePolicy.allowed_models.join("\n")}
                onChange={(event) =>
                  onEnterprisePolicyChange({
                    ...enterprisePolicy,
                    allowed_models: event.target.value
                      .split(/\r?\n/)
                      .map((item) => item.trim())
                      .filter(Boolean)
                  })
                }
                className="min-h-[96px] w-full rounded-lg border border-transparent bg-transparent px-3 py-2 text-xs text-[var(--text-primary)] outline-none transition hover:bg-[var(--accent-soft)] focus:bg-[var(--accent-soft)] focus:ring-1 focus:ring-[var(--line-soft-focus)]"
                placeholder="nomic-embed-text:latest"
              />
            </div>
          </div>
          <div className="grid gap-2 text-xs text-[var(--text-secondary)] md:grid-cols-2">
            <div>
              {t("policyCurrentMode", {
                mode:
                  enterprisePolicy.egress_mode === "local_only"
                    ? t("policyModeLocalOnly")
                    : t("policyModeAllowlist")
              })}
            </div>
            <div>{t("policyEndpointCount", { count: enterprisePolicy.allowed_model_endpoints.length })}</div>
            <div>{t("policyModelCount", { count: enterprisePolicy.allowed_models.length })}</div>
            <div className={activeProviderPolicyBlock ? "text-amber-300" : "text-[var(--text-secondary)]"}>
              {activeProviderPolicyBlock ?? t("policyStatusAllowed")}
            </div>
          </div>
        </AnimatedPanel>

        <SettingCard title={t("modelEndpoint")}>
          <div className="w-[320px]">
            <CyberInput
              value={activeProfile.endpoint}
              onChange={(value) => updateActiveProfile({ endpoint: value })}
              placeholder={activeProvider === "ollama_local" ? "http://localhost:11434" : "https://api.openai.com"}
            />
          </div>
        </SettingCard>

        {activeProvider === "openai_compatible" ? (
          <SettingCard title={t("modelApiKey")}>
            <div className="w-[320px]">
              <CyberInput
                value={modelSettings.remote_profile.api_key ?? ""}
                onChange={(value) => updateActiveProfile({ api_key: value })}
                placeholder="sk-..."
              />
            </div>
          </SettingCard>
        ) : null}

        {activeProvider === "ollama_local" ? (
          <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-sm text-[var(--text-primary)]">{t("modelLocalRoot")}</div>
                <div className="mt-1 truncate font-mono text-xs text-[var(--text-secondary)]">
                  {modelSettings.local_profile.models_root || "-"}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <AnimatedPressButton
                  type="button"
                  onClick={() => void onModelAction("refresh", onPickLocalModelsRoot)}
                  disabled={modelBusy}
                  className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
                >
                  {t("modelLocalRootPick")}
                </AnimatedPressButton>
                <AnimatedPressButton
                  type="button"
                  onClick={onClearLocalModelsRoot}
                  disabled={modelBusy}
                  className="rounded-md border border-transparent bg-transparent px-3 py-1.5 text-sm text-[var(--text-secondary)] transition hover:bg-[var(--accent-soft)] hover:text-[var(--accent)] disabled:opacity-60"
                >
                  {t("modelLocalRootClear")}
                </AnimatedPressButton>
              </div>
            </div>
          </AnimatedPanel>
        ) : null}

        <AnimatedPanel className="glass-panel-infer rounded-lg px-3 py-3">
          <div className="mb-2 text-sm text-[var(--text-primary)]">{t("modelMergedCandidates")}</div>
          <div className="flex flex-wrap items-center gap-3 text-xs text-[var(--text-secondary)]">
            <span>
              {t("modelFromFolder")}: {providerModels.from_folder.length}
            </span>
            <span>
              {t("modelFromService")}: {providerModels.from_service.length}
            </span>
            <span>
              {t("modelMergedCandidates")}: {providerModels.merged.length}
            </span>
          </div>
          {providerModels.merged.length === 0 ? (
            <div className="mt-2 text-xs text-[var(--text-secondary)]">{t("modelNoCandidates")}</div>
          ) : null}
        </AnimatedPanel>

        <ModelRoleSelector
          label={t("chatModel")}
          value={activeProfile.chat_model}
          options={providerModels.merged}
          customMode={customMode.chat_model}
          onToggleCustom={() => setCustomMode((prev) => ({ ...prev, chat_model: !prev.chat_model }))}
          onChange={(value) => updateActiveProfile({ chat_model: value })}
        />
        <ModelRoleSelector
          label={t("graphModel")}
          value={activeProfile.graph_model}
          options={providerModels.merged}
          customMode={customMode.graph_model}
          onToggleCustom={() => setCustomMode((prev) => ({ ...prev, graph_model: !prev.graph_model }))}
          onChange={(value) => updateActiveProfile({ graph_model: value })}
        />
        <ModelRoleSelector
          label={t("embedModel")}
          value={activeProfile.embed_model}
          options={providerModels.merged}
          customMode={customMode.embed_model}
          onToggleCustom={() => setCustomMode((prev) => ({ ...prev, embed_model: !prev.embed_model }))}
          onChange={(value) => updateActiveProfile({ embed_model: value })}
        />

        <AnimatedPanel className="glass-panel-infer space-y-2 rounded-md px-3 py-2 text-xs text-[var(--text-secondary)]">
          <div className="text-[var(--text-primary)]">{t("modelStatusTitle")}</div>
          <div className="flex flex-wrap gap-2">
            {(["probe", "refresh", "save"] as ModelActionKey[]).map((key) => (
              <motion.button
                key={key}
                type="button"
                onClick={() =>
                  void onModelAction(
                    key,
                    key === "probe" ? onProbeModelProvider : key === "refresh" ? onRefreshProviderModels : onSaveModelSettings
                  )
                }
                disabled={modelBusy}
                className={`${stableActionButtonClass} ${buttonClassByState(key)}`}
                animate={actionState[key].phase === "success" ? { scale: [1, 1.015, 1] } : { scale: 1 }}
                transition={{ duration: 0.35 }}
              >
                {actionState[key].phase === "running" ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : null}
                {buttonLabelByState(key)}
              </motion.button>
            ))}
            {activeProvider === "ollama_local" ? (
              <motion.button
                key="pull"
                type="button"
                onClick={() => {
                  const candidates = modelAvailability?.missing_roles ?? [];
                  if (candidates.includes("embed")) {
                    void onModelAction("pull", () => onPullModel(activeProfile.embed_model));
                  } else if (candidates.includes("chat")) {
                    void onModelAction("pull", () => onPullModel(activeProfile.chat_model));
                  } else if (candidates.includes("graph")) {
                    void onModelAction("pull", () => onPullModel(activeProfile.graph_model));
                  }
                }}
                disabled={modelBusy || !modelAvailability?.missing_roles?.length}
                className={`${stableActionButtonClass} ${buttonClassByState("pull")}`}
                animate={actionState.pull.phase === "success" ? { scale: [1, 1.015, 1] } : { scale: 1 }}
                transition={{ duration: 0.35 }}
              >
                {actionState.pull.phase === "running" ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : null}
                {buttonLabelByState("pull")}
              </motion.button>
            ) : null}
          </div>
          {modelAvailability ? (
            <Fragment>
              <div>{modelAvailability.reachable ? t("modelStatusReachable") : t("modelStatusUnreachable")}</div>
              <div>
                {modelAvailability.missing_roles.length > 0
                  ? t("modelStatusMissing", { roles: modelAvailability.missing_roles.join(", ") })
                  : t("modelStatusReady")}
              </div>
              {modelAvailability.checked_provider ? (
                <div>
                  {t("modelStatusProvider", {
                    provider:
                      modelAvailability.checked_provider === "ollama_local" ? t("providerOllama") : t("providerOpenAI")
                  })}
                </div>
              ) : null}
              {modelAvailability.errors.map((item, idx) => (
                <div key={`${item.code}-${idx}`} className="text-red-300">
                  {item.code}: {item.message}
                </div>
              ))}
            </Fragment>
          ) : null}
        </AnimatedPanel>
      </motion.div>
    </motion.div>
  );
}
