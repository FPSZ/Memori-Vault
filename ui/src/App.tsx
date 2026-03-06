import { FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { Database, LoaderCircle, Sparkles, Terminal } from "lucide-react";

type VaultStats = {
  documents: number;
  chunks: number;
  nodes: number;
};

type VaultStatsRaw = Partial<
  VaultStats & {
    document_count: number;
    chunk_count: number;
    graph_node_count: number;
  }
>;

function normalizeStats(raw: VaultStatsRaw): VaultStats {
  return {
    documents: raw.documents ?? raw.document_count ?? 0,
    chunks: raw.chunks ?? raw.chunk_count ?? 0,
    nodes: raw.nodes ?? raw.graph_node_count ?? 0
  };
}

export default function App() {
  const [query, setQuery] = useState("");
  const [answer, setAnswer] = useState("");
  const [loading, setLoading] = useState(false);
  const [stats, setStats] = useState<VaultStats>({ documents: 0, chunks: 0, nodes: 0 });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    const loadStats = async () => {
      try {
        const raw = await invoke<VaultStatsRaw>("get_vault_stats");
        if (active) {
          setStats(normalizeStats(raw));
        }
      } catch (err) {
        if (active) {
          setError(`统计信息加载失败：${String(err)}`);
        }
      }
    };

    loadStats();
    const timer = window.setInterval(loadStats, 5000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const canSubmit = useMemo(() => query.trim().length > 0 && !loading, [query, loading]);

  const onSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const text = await invoke<string>("ask_vault", { query: query.trim() });
      setAnswer(text);
    } catch (err) {
      setAnswer("");
      setError(`检索失败：${String(err)}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="relative min-h-screen bg-[#0d1117] text-[#c9d1d9]">
      <div className="pointer-events-none absolute inset-0 overflow-hidden">
        <div className="absolute left-1/2 top-[-240px] h-[580px] w-[580px] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(88,166,255,0.16),rgba(88,166,255,0)_70%)]" />
      </div>

      <main className="relative mx-auto flex min-h-screen w-full max-w-5xl flex-col px-6 pb-24 pt-16 md:px-10">
        <header className="mb-10 space-y-3">
          <p className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.03] px-3 py-1 text-xs tracking-[0.18em] text-[#8b949e] uppercase">
            <Terminal size={14} className="text-[#58a6ff]" />
            Memori-Vault / Phase 3
          </p>
          <h1 className="text-4xl font-semibold tracking-tight text-[#f0f6fc] md:text-5xl">
            Omnibar
          </h1>
          <p className="max-w-2xl text-sm text-[#8b949e] md:text-base">
            本地 Graph-RAG 引擎，键入问题并直接调用 Tauri IPC 获取合成答案。
          </p>
        </header>

        <form onSubmit={onSubmit} className="mb-8">
          <div className="group flex items-center rounded-2xl border border-white/10 bg-[#0b1016]/95 px-5 py-4 shadow-[0_0_0_1px_rgba(88,166,255,0.06)] transition-colors focus-within:border-[#58a6ff]/70">
            <Sparkles className="mr-3 h-5 w-5 text-[#58a6ff]" />
            <input
              type="text"
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="例如：苹果公司是谁创立的？"
              className="w-full border-none bg-transparent text-lg text-[#f0f6fc] outline-none placeholder:text-[#6e7681]"
            />
          </div>
          <div className="mt-3 flex items-center justify-between text-xs text-[#8b949e]">
            <span>Enter 提交检索</span>
            <button
              type="submit"
              disabled={!canSubmit}
              className="rounded-lg border border-[#58a6ff]/40 px-3 py-1 text-[#58a6ff] transition-opacity disabled:cursor-not-allowed disabled:opacity-40"
            >
              Ask Vault
            </button>
          </div>
        </form>

        <section className="flex-1">
          {loading && (
            <motion.div
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              className="flex items-center gap-3 rounded-xl border border-white/10 bg-white/[0.02] px-4 py-3 text-sm text-[#8b949e]"
            >
              <LoaderCircle className="h-4 w-4 animate-spin text-[#58a6ff]" />
              正在调用 Memori 引擎并合成答案...
            </motion.div>
          )}

          {!loading && error && (
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              className="rounded-xl border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-300"
            >
              {error}
            </motion.div>
          )}

          {!loading && !error && answer && (
            <motion.article
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.28, ease: "easeOut" }}
              className="rounded-2xl border border-white/10 bg-[#0b1016] p-6 shadow-[0_20px_60px_rgba(0,0,0,0.35)]"
            >
              <h2 className="mb-4 flex items-center gap-2 text-base font-medium text-[#f0f6fc]">
                <Database size={16} className="text-[#58a6ff]" />
                合成答案
              </h2>
              <pre className="whitespace-pre-wrap break-words text-sm leading-7 text-[#c9d1d9]">
                {answer}
              </pre>
            </motion.article>
          )}

          {!loading && !error && !answer && (
            <div className="rounded-xl border border-dashed border-white/10 bg-white/[0.02] px-5 py-4 text-sm text-[#6e7681]">
              结果区已就绪。输入问题后将显示合成答案与引用信息。
            </div>
          )}
        </section>
      </main>

      <footer className="fixed inset-x-0 bottom-0 border-t border-white/10 bg-[#0b0f14]/90 backdrop-blur">
        <div className="mx-auto flex h-11 w-full max-w-5xl items-center justify-between px-6 text-xs text-[#8b949e] md:px-10">
          <span>
            Vault: {stats.documents} Docs / {stats.chunks} Chunks / {stats.nodes} Nodes
          </span>
          <span className="text-[#58a6ff]">local-first</span>
        </div>
      </footer>
    </div>
  );
}
