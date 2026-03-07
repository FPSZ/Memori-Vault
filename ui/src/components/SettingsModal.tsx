import { motion } from "framer-motion";
import { Cpu, Database, Settings, X } from "lucide-react";
import { useMemo, useState } from "react";
import { CyberInput, CyberToggle } from "./UI";

type SettingsModalProps = {
  onClose: () => void;
};

type TabKey = "engine" | "models" | "advanced";

export function SettingsModal({ onClose }: SettingsModalProps) {
  const [activeTab, setActiveTab] = useState<TabKey>("engine");
  const [autoSyncDaemon, setAutoSyncDaemon] = useState(true);
  const [graphRagInfer, setGraphRagInfer] = useState(true);
  const [graphModel, setGraphModel] = useState("qwen2.5:7b");

  const tabMeta = useMemo(
    () => [
      { key: "engine" as const, label: "Engine", icon: Cpu },
      { key: "models" as const, label: "Models", icon: Settings },
      { key: "advanced" as const, label: "Advanced", icon: Database }
    ],
    []
  );

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-md"
      onClick={onClose}
    >
      <motion.div
        initial={{ scale: 0.95, opacity: 0, y: 10 }}
        animate={{ scale: 1, opacity: 1, y: 0 }}
        transition={{ type: "spring", damping: 25, stiffness: 300 }}
        className="flex h-[400px] w-[600px] overflow-hidden rounded-2xl border border-white/10 bg-[#161b22]/90 shadow-2xl backdrop-blur-3xl"
        onClick={(e) => e.stopPropagation()}
      >
        <aside className="w-1/3 border-r border-white/10 bg-[#0d1117]/50 p-3">
          <div className="mb-3 px-2 pt-1 text-xs font-mono tracking-[0.16em] text-[#8b949e] uppercase">
            Settings
          </div>
          <div className="space-y-1">
            {tabMeta.map((tab) => {
              const Icon = tab.icon;
              const active = activeTab === tab.key;
              return (
                <button
                  key={tab.key}
                  type="button"
                  onClick={() => setActiveTab(tab.key)}
                  className={`relative flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                    active ? "text-[#58a6ff]" : "text-[#8b949e] hover:text-[#c9d1d9]"
                  }`}
                >
                  {active && <span className="absolute left-0 h-4 w-[2px] rounded bg-[#58a6ff]" />}
                  <Icon className="h-4 w-4" />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </div>
        </aside>

        <section className="relative flex-1 p-5">
          <button
            type="button"
            onClick={onClose}
            className="absolute right-3 top-3 inline-flex h-7 w-7 items-center justify-center rounded-md text-[#8b949e] transition hover:bg-[#21262d] hover:text-[#c9d1d9]"
            aria-label="Close settings"
            title="Close"
          >
            <X className="h-4 w-4" />
          </button>

          {activeTab === "engine" && (
            <div className="pt-6">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[#58a6ff] uppercase">
                Engine
              </h3>

              <div className="space-y-4">
                <div className="flex items-center justify-between rounded-lg border border-[#30363d] bg-[#0d1117]/40 px-3 py-3">
                  <div>
                    <div className="text-sm text-[#c9d1d9]">Auto-Sync Daemon</div>
                    <div className="mt-1 text-xs text-[#8b949e]">Enable background file watching.</div>
                  </div>
                  <CyberToggle
                    checked={autoSyncDaemon}
                    onChange={setAutoSyncDaemon}
                    ariaLabel="Auto-Sync Daemon"
                  />
                </div>

                <div className="flex items-center justify-between rounded-lg border border-[#30363d] bg-[#0d1117]/40 px-3 py-3">
                  <div>
                    <div className="text-sm text-[#c9d1d9]">Graph-RAG Infer</div>
                    <div className="mt-1 text-xs text-[#8b949e]">
                      Enable graph context inference in answer synthesis.
                    </div>
                  </div>
                  <CyberToggle
                    checked={graphRagInfer}
                    onChange={setGraphRagInfer}
                    ariaLabel="Graph-RAG Infer"
                  />
                </div>
              </div>
            </div>
          )}

          {activeTab === "models" && (
            <div className="pt-6">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[#58a6ff] uppercase">
                Models
              </h3>

              <label className="mb-2 block text-xs font-mono tracking-wide text-[#8b949e]">
                Graph Extractor Model
              </label>
              <CyberInput value={graphModel} onChange={setGraphModel} placeholder="qwen2.5:7b" />
              <p className="mt-3 text-xs leading-5 text-[#8b949e]">
                Used for entity and relation extraction. Prefer `qwen2.5:7b` for stable JSON output.
              </p>
            </div>
          )}

          {activeTab === "advanced" && (
            <div className="pt-6">
              <h3 className="mb-5 text-sm font-mono tracking-[0.16em] text-[#58a6ff] uppercase">
                Advanced
              </h3>
              <div className="rounded-lg border border-[#30363d] bg-[#0d1117]/40 p-3 text-xs leading-5 text-[#8b949e]">
                Advanced runtime controls and diagnostics hooks will be exposed here in Phase 3.
              </div>
            </div>
          )}
        </section>
      </motion.div>
    </motion.div>
  );
}
