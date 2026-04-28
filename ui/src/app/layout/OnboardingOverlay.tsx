import { AnimatePresence, motion } from "framer-motion";
import { useI18n } from "../../i18n";
import type {
  LocalModelProfileDto,
  ModelAvailabilityDto,
  ModelProvider,
  ModelSettingsDto,
  ProviderModelsDto,
  RemoteModelProfileDto
} from "../../components/settings/types";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type OnboardingOverlayProps = {
  open: boolean;
  t: TranslateFn;
  onboardingStep: number;
  onClose: () => void;
  onStepBack: () => void;
  onStepNext: () => void;
  onFinish: () => void;
  onSelectProvider: (provider: ModelProvider) => void;
  modelSettings: ModelSettingsDto;
  activeModelProfile: LocalModelProfileDto | RemoteModelProfileDto;
  updateActiveOnboardingProfile: (next: Partial<LocalModelProfileDto & RemoteModelProfileDto>) => void;
  providerModels: ProviderModelsDto;
  modelAvailability: ModelAvailabilityDto | null;
  modelBusy: boolean;
  modelSetupReady: boolean;
  onProbeModelProvider: () => Promise<void>;
  onRefreshProviderModels: () => Promise<void>;
};

export function OnboardingOverlay({
  open,
  t,
  onboardingStep,
  onClose,
  onStepBack,
  onStepNext,
  onFinish,
  onSelectProvider,
  modelSettings,
  activeModelProfile,
  updateActiveOnboardingProfile,
  providerModels,
  modelAvailability,
  modelBusy,
  modelSetupReady,
  onProbeModelProvider,
  onRefreshProviderModels
}: OnboardingOverlayProps) {
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="absolute inset-0 z-40 flex items-center justify-center bg-[var(--overlay)] backdrop-blur-sm"
        >
          <motion.div
            initial={{ opacity: 0, y: 12, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 10, scale: 0.98 }}
            transition={{ type: "spring", damping: 24, stiffness: 220 }}
            className="w-[680px] rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface-1)] p-5 shadow-2xl"
          >
            <div className="mb-4 flex items-center justify-between">
              <div className="text-sm tracking-[0.14em] text-[var(--accent)] uppercase">{t("setupWizard")}</div>
              <button
                type="button"
                onClick={onClose}
                className="text-xs text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
              >
                {t("closeWizard")}
              </button>
            </div>

            <div className="mb-4 text-xs text-[var(--text-secondary)]">Step {onboardingStep + 1}/4</div>

            {onboardingStep === 0 && (
              <div className="space-y-3">
                <div className="text-sm text-[var(--text-primary)]">{t("modelProvider")}</div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => onSelectProvider("llama_cpp_local")}
                    className={`rounded-md border px-3 py-2 text-xs transition ${
                      modelSettings.active_provider === "llama_cpp_local"
                        ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                        : "border-[var(--border-strong)] text-[var(--text-secondary)]"
                    }`}
                  >
                    {t("providerLocal")}
                  </button>
                  <button
                    type="button"
                    onClick={() => onSelectProvider("openai_compatible")}
                    className={`rounded-md border px-3 py-2 text-xs transition ${
                      modelSettings.active_provider === "openai_compatible"
                        ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                        : "border-[var(--border-strong)] text-[var(--text-secondary)]"
                    }`}
                  >
                    {t("providerOpenAI")}
                  </button>
                </div>
              </div>
            )}

            {onboardingStep === 1 && (
              <div className="space-y-3">
                <div>
                  <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("chatEndpoint")}</div>
                  <input
                    value={activeModelProfile.chat_endpoint}
                    onChange={(e) => updateActiveOnboardingProfile({ chat_endpoint: e.target.value })}
                    className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                  />
                </div>
                {modelSettings.active_provider === "openai_compatible" && (
                  <div>
                    <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("modelApiKey")}</div>
                    <input
                      value={modelSettings.remote_profile.api_key ?? ""}
                      onChange={(e) => updateActiveOnboardingProfile({ api_key: e.target.value })}
                      className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                    />
                  </div>
                )}
              </div>
            )}

            {onboardingStep === 2 && (
              <div className="space-y-3">
                <div>
                  <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("chatModel")}</div>
                  <select
                    value={activeModelProfile.chat_model}
                    onChange={(e) => updateActiveOnboardingProfile({ chat_model: e.target.value })}
                    className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                  >
                    {providerModels.merged.length === 0 ? (
                      <option value={activeModelProfile.chat_model}>{activeModelProfile.chat_model}</option>
                    ) : null}
                    {!providerModels.merged.includes(activeModelProfile.chat_model) ? (
                      <option value={activeModelProfile.chat_model}>{activeModelProfile.chat_model}</option>
                    ) : null}
                    {providerModels.merged.map((item) => (
                      <option key={`onboard-chat-${item}`} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("graphModel")}</div>
                  <select
                    value={activeModelProfile.graph_model}
                    onChange={(e) => updateActiveOnboardingProfile({ graph_model: e.target.value })}
                    className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                  >
                    {providerModels.merged.length === 0 ? (
                      <option value={activeModelProfile.graph_model}>{activeModelProfile.graph_model}</option>
                    ) : null}
                    {!providerModels.merged.includes(activeModelProfile.graph_model) ? (
                      <option value={activeModelProfile.graph_model}>{activeModelProfile.graph_model}</option>
                    ) : null}
                    {providerModels.merged.map((item) => (
                      <option key={`onboard-graph-${item}`} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <div className="mb-1 text-xs text-[var(--text-secondary)]">{t("embedModel")}</div>
                  <select
                    value={activeModelProfile.embed_model}
                    onChange={(e) => updateActiveOnboardingProfile({ embed_model: e.target.value })}
                    className="w-full rounded-lg border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-sm text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
                  >
                    {providerModels.merged.length === 0 ? (
                      <option value={activeModelProfile.embed_model}>{activeModelProfile.embed_model}</option>
                    ) : null}
                    {!providerModels.merged.includes(activeModelProfile.embed_model) ? (
                      <option value={activeModelProfile.embed_model}>{activeModelProfile.embed_model}</option>
                    ) : null}
                    {providerModels.merged.map((item) => (
                      <option key={`onboard-embed-${item}`} value={item}>
                        {item}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            )}

            {onboardingStep === 3 && (
              <div className="space-y-3">
                <div className="text-sm text-[var(--text-primary)]">{t("testConnection")}</div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => void onProbeModelProvider()}
                    disabled={modelBusy}
                    className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                  >
                    {t("testConnection")}
                  </button>
                  <button
                    type="button"
                    onClick={() => void onRefreshProviderModels()}
                    disabled={modelBusy}
                    className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-primary)] transition hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-60"
                  >
                    {t("refreshModels")}
                  </button>
                </div>
                <div className="rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface-2)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                  {modelAvailability?.reachable ? t("modelStatusReachable") : t("modelStatusUnreachable")}
                  {" | "}
                  {modelAvailability?.missing_roles?.length
                    ? t("modelStatusMissing", { roles: modelAvailability.missing_roles.join(", ") })
                    : t("modelStatusReady")}
                </div>
              </div>
            )}

            <div className="mt-5 flex items-center justify-between">
              <button
                type="button"
                onClick={onStepBack}
                disabled={onboardingStep === 0}
                className="rounded-md border border-[var(--border-strong)] bg-[var(--bg-surface-2)] px-3 py-1.5 text-xs text-[var(--text-primary)] disabled:opacity-50"
              >
                {t("previousStep")}
              </button>
              {onboardingStep < 3 ? (
                <button
                  type="button"
                  onClick={onStepNext}
                  className="rounded-md border border-[var(--accent)] bg-[var(--accent-soft)] px-3 py-1.5 text-xs text-[var(--accent)]"
                >
                  {t("nextStep")}
                </button>
              ) : (
                <button
                  type="button"
                  onClick={onFinish}
                  disabled={!modelSetupReady || modelBusy}
                  className="rounded-md border border-[var(--accent)] bg-[var(--accent-soft)] px-3 py-1.5 text-xs text-[var(--accent)]"
                >
                  {t("finishSetup")}
                </button>
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
