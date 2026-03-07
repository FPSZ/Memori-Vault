import { CSSProperties, KeyboardEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion } from "framer-motion";
import {
  Check,
  ChevronDown,
  ChevronUp,
  FolderOpen,
  LoaderCircle,
  Minus,
  Search,
  Sparkles,
  Square,
  X
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

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

type Source = {
  score: number;
  path: string;
  content: string;
};

type ParsedResponse = {
  synthesis: string;
  sources: Source[];
};

const TAURI_HOST_MISSING_MESSAGE = "未检测到 Tauri 宿主环境，请使用 cargo tauri dev 启动";

function isTauriHostAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const w = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

function toUiErrorMessage(error: unknown): string {
  if (!isTauriHostAvailable()) {
    return TAURI_HOST_MISSING_MESSAGE;
  }

  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (error && typeof error === "object") {
    const maybeMessage = (error as { message?: unknown }).message;
    if (typeof maybeMessage === "string" && maybeMessage.trim()) {
      return maybeMessage;
    }
  }

  return "调用后端命令失败，请检查桌面端日志。";
}

function normalizeStats(raw: VaultStatsRaw): VaultStats {
  return {
    documents: raw.documents ?? raw.document_count ?? 0,
    chunks: raw.chunks ?? raw.chunk_count ?? 0,
    nodes: raw.nodes ?? raw.graph_node_count ?? 0
  };
}

function parseResponse(raw: string): ParsedResponse {
  const normalized = raw.replace(/\r\n/g, "\n");
  const parts = normalized.split("---");
  let synthesis = (parts.shift() ?? "").trim();
  let sourceSection = parts.join("---").replace(/参考来源[:：]/g, "").trim();

  // Fallback format: answer generation failed and backend directly returns
  // "本地大模型答案生成失败... + references" without the '---' separator.
  if (!sourceSection) {
    const firstRefIndex = normalized.search(/^\s*#\d+\s+相似度[:：]/m);
    if (firstRefIndex >= 0) {
      synthesis = normalized.slice(0, firstRefIndex).trim();
      sourceSection = normalized.slice(firstRefIndex).trim();
    }
  }

  if (!sourceSection) {
    return { synthesis, sources: [] };
  }

  const chunkBlocks = sourceSection
    .split(/(?=\n?#\d+\s+相似度[:：])/)
    .map((block) => block.trim())
    .filter((block) => block.length > 0);

  const sources: Source[] = [];

  for (const block of chunkBlocks) {
    const scoreMatch = block.match(/相似度[:：]\s*([0-9]*\.?[0-9]+)/);
    const pathMatch = block.match(/来源[:：]\s*(.+)/);
    const labeledContentMatch = block.match(/内容[:：]?\s*\n([\s\S]*)/);

    if (!scoreMatch || !pathMatch) {
      continue;
    }

    const score = Number.parseFloat(scoreMatch[1]);
    if (!Number.isFinite(score)) {
      continue;
    }

    const path = pathMatch[1].trim();
    let content = "";
    if (labeledContentMatch) {
      content = labeledContentMatch[1].trim();
    } else {
      // Current backend format has no explicit "内容:" label.
      // Strip metadata lines and keep the remaining text as snippet.
      content = block
        .split("\n")
        .filter((line) => {
          const trimmed = line.trim();
          if (!trimmed) return false;
          if (/^#\d+\s+相似度[:：]/.test(trimmed)) return false;
          if (/^来源[:：]/.test(trimmed)) return false;
          if (/^块序号[:：]/.test(trimmed)) return false;
          if (/^-{8,}$/.test(trimmed)) return false;
          return true;
        })
        .join("\n")
        .trim();
    }

    if (!path || !content) {
      continue;
    }

    sources.push({ score, path, content });
  }

  return { synthesis, sources };
}

function clampScore(score: number): number {
  if (!Number.isFinite(score)) {
    return 0;
  }
  return Math.max(0, Math.min(1, score));
}

function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path.trim());
}

function LiquidOrb({ score }: { score: number }) {
  const normalized = clampScore(score);
  const percentage = Math.round(normalized * 100);
  const liquidTop = `${100 - percentage}%`;

  return (
    <div className="flex items-center gap-3">
      <div className="glass-orb" style={{ "--liquid-top": liquidTop } as CSSProperties}>
        <span className="glass-orb-light" />
        <div className="glass-orb-fluid">
          <span className="glass-orb-liquid">
            <span className="glass-orb-foam" />
          </span>
          <span className="glass-orb-liquid glass-orb-liquid-back">
            <span className="glass-orb-foam" />
          </span>
        </div>
      </div>

      <div className="flex flex-col items-start gap-1 leading-none">
        <span className="text-sm font-mono font-bold text-[#58a6ff] drop-shadow-[0_0_5px_rgba(88,166,255,0.8)]">
          {percentage}%
        </span>
        <span className="text-[10px] tracking-[0.08em] text-[#8b949e]">语义相关度</span>
      </div>
    </div>
  );
}

export default function App() {
  const [query, setQuery] = useState("");
  const [rawAnswer, setRawAnswer] = useState("");
  const [loading, setLoading] = useState(false);
  const [isSearching, setIsSearching] = useState(false);
  const [isMaximized, setIsMaximized] = useState(false);
  const [expandedSourceKeys, setExpandedSourceKeys] = useState<Set<string>>(() => new Set());
  const [stats, setStats] = useState<VaultStats>({ documents: 0, chunks: 0, nodes: 0 });
  const [error, setError] = useState<string | null>(null);

  const parsed = useMemo(() => parseResponse(rawAnswer), [rawAnswer]);
  const canSubmit = useMemo(() => query.trim().length > 0 && !loading, [loading, query]);
  const showSearchDone = useMemo(
    () => isSearching && !loading && !error && rawAnswer.trim().length > 0,
    [error, isSearching, loading, rawAnswer]
  );

  useEffect(() => {
    let active = true;

    const loadStats = async () => {
      try {
        const raw = await invoke<VaultStatsRaw>("get_vault_stats");
        if (active) {
          setStats(normalizeStats(raw));
        }
      } catch (error) {
        if (active) {
          setError(toUiErrorMessage(error));
        }
      }
    };

    void loadStats();
    const timer = window.setInterval(() => {
      void loadStats();
    }, 5000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    let mounted = true;

    const syncMaximizeState = async () => {
      if (!isTauriHostAvailable()) {
        return;
      }

      try {
        const maximized = await getCurrentWindow().isMaximized();
        if (mounted) {
          setIsMaximized(maximized);
        }
      } catch {
        // Ignore; this is best-effort UI sync.
      }
    };

    void syncMaximizeState();

    return () => {
      mounted = false;
    };
  }, []);

  const runSearch = async () => {
    if (!canSubmit) {
      return;
    }

    setIsSearching(true);
    setLoading(true);
    setError(null);
    setExpandedSourceKeys(new Set());

    try {
      const text = await invoke<string>("ask_vault", { query: query.trim() });
      setRawAnswer(text);
    } catch (error) {
      setRawAnswer("");
      setError(toUiErrorMessage(error));
    } finally {
      setLoading(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void runSearch();
    }
  };

  const onMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onToggleMaximize = async () => {
    try {
      const win = getCurrentWindow();
      await win.toggleMaximize();
      const maximized = await win.isMaximized();
      setIsMaximized(maximized);
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const onClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  const toggleSourceExpanded = (key: string) => {
    setExpandedSourceKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const onOpenSourceLocation = async (path: string) => {
    try {
      await invoke("open_source_location", { path });
    } catch (err) {
      setError(toUiErrorMessage(err));
    }
  };

  return (
    <div className="h-screen w-screen bg-[#0d1117] text-[#c9d1d9]">
      <div className="relative flex h-full w-full flex-col overflow-hidden bg-[#0d1117] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.08)]">
        <header
          data-tauri-drag-region=""
          className="relative z-10 flex h-9 shrink-0 items-center pl-2 pr-2 select-none [app-region:drag] [-webkit-app-region:drag]"
        >
          <div data-tauri-drag-region="" className="h-full flex-1 cursor-move" />
          <div className="flex items-center gap-1.5 [app-region:no-drag] [-webkit-app-region:no-drag]">
            <button
              type="button"
              onClick={() => void onMinimize()}
              className="inline-flex items-center justify-center p-1 text-[#8b949e] transition hover:text-[#c9d1d9]"
              aria-label="Minimize"
              title="Minimize"
            >
              <Minus className="h-4 w-4" />
            </button>
            <button
              type="button"
              onClick={() => void onToggleMaximize()}
              className="inline-flex items-center justify-center p-1 text-[#8b949e] transition hover:text-[#c9d1d9]"
              aria-label={isMaximized ? "Restore" : "Maximize"}
              title={isMaximized ? "Restore" : "Maximize"}
            >
              <Square className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={() => void onClose()}
              className="inline-flex items-center justify-center p-1 text-[#ff7b72] transition hover:text-[#ffb4ad]"
              aria-label="Close"
              title="Close"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </header>

        <div className="relative z-10 flex-1 overflow-hidden">
          <div className="pointer-events-none absolute inset-0 overflow-hidden">
            <div className="absolute left-1/2 top-[-220px] h-[620px] w-[620px] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(88,166,255,0.14),rgba(88,166,255,0)_72%)]" />
          </div>

          <main className="relative mx-auto h-full w-full max-w-5xl px-6 pb-4 md:px-10">
            <motion.div
              className="absolute left-6 right-6 md:left-10 md:right-10"
              animate={{
                top: isSearching ? "20px" : "45%",
                y: isSearching ? 0 : "-50%"
              }}
              transition={{ type: "spring", stiffness: 180, damping: 24 }}
            >
              <div className="relative mx-auto w-full max-w-4xl rounded-xl bg-[#161b22] px-6 py-5 ring-1 ring-white/10">
                <Search className="pointer-events-none absolute left-6 top-1/2 h-5 w-5 -translate-y-1/2 text-[#8b949e]" />
                <input
                  type="text"
                  autoFocus
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={onKeyDown}
                  placeholder="Ask your vault..."
                  className="w-full border-none bg-transparent pl-9 pr-10 text-2xl text-[#c9d1d9] placeholder:text-[#6e7681] focus:outline-none focus:ring-0"
                />
                <AnimatePresence>
                  {isSearching && loading && (
                    <motion.div
                      initial={{ opacity: 0.2, scale: 0.95 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ repeat: Infinity, repeatType: "reverse", duration: 0.9 }}
                      className="absolute right-6 top-1/2 -translate-y-1/2 text-[#58a6ff]"
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
                      className="absolute right-6 top-1/2 -translate-y-1/2 text-[#58a6ff]"
                    >
                      <Check className="h-5 w-5" />
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            </motion.div>

            <AnimatePresence>
              {isSearching && (
                <motion.section
                  initial={{ opacity: 0, y: 20 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 12 }}
                  transition={{ duration: 0.24, ease: "easeOut" }}
                  className="mx-auto h-full w-full max-w-4xl overflow-y-auto pt-36"
                >
                  {loading && (
                    <div className="flex items-center gap-3 rounded-xl border border-white/10 bg-[#11161d] px-5 py-4 text-sm text-[#9aa4af]">
                      <LoaderCircle className="h-4 w-4 animate-spin text-[#58a6ff]" />
                      Retrieving and synthesizing...
                    </div>
                  )}

                  {!loading && error && (
                    <div className="rounded-xl border border-red-500/40 bg-red-500/10 px-5 py-4 text-sm text-red-300">
                      {error}
                    </div>
                  )}

                  {!loading && !error && parsed.synthesis && (
                    <article className="pb-8">
                      <div className="mb-6 border-l-2 border-[#58a6ff] pl-4">
                        <div className="mb-3 flex items-center gap-2">
                          <Sparkles className="h-4 w-4 text-[#58a6ff]" />
                          <span className="text-xs font-mono font-bold tracking-widest text-[#58a6ff]">
                            SYNTHESIS
                          </span>
                        </div>
                        <p className="whitespace-pre-wrap break-words font-sans text-lg leading-relaxed text-[#c9d1d9]">
                          {parsed.synthesis}
                        </p>
                      </div>

                      {parsed.sources.length > 0 && (
                        <section className="mt-8">
                          <div className="mb-3 text-[11px] tracking-[0.16em] text-[#8b949e] uppercase">
                            CONTEXT SOURCES
                          </div>

                          <div className="grid grid-cols-1 items-start gap-3 md:grid-cols-2">
                            {parsed.sources.map((source, index) => {
                              const sourceKey = `${source.path}-${index}`;
                              const expanded = expandedSourceKeys.has(sourceKey);
                              const markdownPreview = expanded && isMarkdownFile(source.path);

                              return (
                                <div
                                  key={sourceKey}
                                  className="relative h-fit self-start flex flex-row items-start gap-4 rounded-xl border border-[#30363d] bg-[#0d1117] p-4"
                                >
                                  <LiquidOrb score={source.score} />

                                  <div className="min-w-0 flex-1 pr-5">
                                    <div className="truncate font-mono text-xs text-[#8b949e]">
                                      {source.path}
                                    </div>
                                    {markdownPreview ? (
                                      <div className="md-preview mt-2 text-sm leading-6 text-[#8b949e]">
                                        <ReactMarkdown remarkPlugins={[remarkGfm]}>
                                          {source.content}
                                        </ReactMarkdown>
                                      </div>
                                    ) : (
                                      <p
                                        className={
                                          expanded
                                            ? "mt-2 whitespace-pre-wrap break-words text-sm leading-6 text-[#6e7681]"
                                            : "mt-2 overflow-hidden text-sm leading-6 text-[#6e7681] [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2]"
                                        }
                                      >
                                        {source.content}
                                      </p>
                                    )}
                                  </div>

                                  <button
                                    type="button"
                                    onClick={() => void onOpenSourceLocation(source.path)}
                                    className="absolute top-3 right-8 p-0 text-[#8b949e] transition hover:text-[#58a6ff]"
                                    aria-label="打开文件位置"
                                    title="打开文件位置"
                                  >
                                    <FolderOpen className="h-4 w-4" />
                                  </button>

                                  <button
                                    type="button"
                                    onClick={() => toggleSourceExpanded(sourceKey)}
                                    className="absolute top-3 right-3 p-0 text-[#8b949e] transition hover:text-[#58a6ff]"
                                    aria-label={expanded ? "收起来源内容" : "展开来源内容"}
                                    title={expanded ? "收起" : "展开"}
                                  >
                                    {expanded ? (
                                      <ChevronUp className="h-4 w-4" />
                                    ) : (
                                      <ChevronDown className="h-4 w-4" />
                                    )}
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        </section>
                      )}
                    </article>
                  )}
                </motion.section>
              )}
            </AnimatePresence>
          </main>
        </div>

        <footer className="relative z-10 shrink-0 border-t border-white/10 bg-[#0f141a]/82 backdrop-blur">
          <div className="mx-auto flex h-8 w-full max-w-5xl items-center justify-between px-6 text-[11px] text-[#8b949e] md:px-10">
            <span>
              Vault: {stats.documents} Docs / {stats.chunks} Chunks / {stats.nodes} Nodes
            </span>
            <span className="inline-flex items-center gap-2 text-[#7ee787]">
              <span className="h-1.5 w-1.5 rounded-full bg-[#3fb950]" />
              Local-First Daemon
            </span>
          </div>
        </footer>
      </div>
    </div>
  );
}
