import { ChevronDown, FolderOpen } from "lucide-react";
import { motion } from "framer-motion";
import ReactMarkdown from "react-markdown";
import { buildCollapsedMarkdownPreview } from "../formatters";
import type { Translate, VisibleCitation } from "../types";

type CitationPanelProps = {
  visibleCitations: VisibleCitation[];
  expandedCitationKeys: Set<string>;
  onToggleCitationExpanded: (key: string) => void;
  onOpenSourceLocation: (path: string) => void;
  markdownRemarkPlugins: any[];
  markdownRehypePlugins: any[];
  t: Translate;
};

export function CitationPanel({
  visibleCitations,
  expandedCitationKeys,
  onToggleCitationExpanded,
  onOpenSourceLocation,
  markdownRemarkPlugins,
  markdownRehypePlugins,
  t
}: CitationPanelProps) {
  if (visibleCitations.length === 0) {
    return null;
  }

  return (
    <section className="mt-6">
      <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">{t("citationsTitle")}</div>
      <div className="space-y-3">
        {visibleCitations.map((citation) => {
          const expanded = expandedCitationKeys.has(citation.citation_key);
          const citationContent = expanded
            ? citation.excerpt
            : buildCollapsedMarkdownPreview(citation.excerpt, 5);

          return (
            <div
              key={citation.citation_key}
              className="relative overflow-hidden rounded-xl border border-[var(--border-strong)] bg-[var(--bg-canvas)] px-4 py-3"
            >
              <div
                aria-hidden="true"
                className="pointer-events-none absolute -left-1 -top-3 z-0 select-none italic text-[88px] font-semibold leading-none text-[color-mix(in_srgb,var(--accent)_16%,transparent)]"
              >
                {citation.index}
              </div>
              <div className="relative z-10 mb-2 flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-xs font-semibold text-[var(--accent)]">
                    {citation.relative_path || citation.file_path}
                  </div>
                  {citation.heading_path.length > 0 ? (
                    <div className="mt-1 text-[11px] text-[var(--text-secondary)]">{citation.heading_path.join(" > ")}</div>
                  ) : null}
                  {citation.duplicate_count > 1 ? (
                    <div className="mt-2 inline-flex rounded-full border border-[var(--border-strong)] px-2 py-0.5 text-[10px] tracking-[0.08em] text-[var(--text-secondary)] uppercase">
                      {t("citationDuplicates", { count: citation.duplicate_count })}
                    </div>
                  ) : null}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => onOpenSourceLocation(citation.file_path)}
                    className="p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                    aria-label={t("openSourceLocation")}
                    title={t("openSourceLocation")}
                  >
                    <FolderOpen className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    onClick={() => onToggleCitationExpanded(citation.citation_key)}
                    className="p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                    aria-label={expanded ? t("collapseCitation") : t("expandCitation")}
                    title={expanded ? t("collapseCitation") : t("expandCitation")}
                  >
                    <motion.span
                      animate={{ rotate: expanded ? 180 : 0 }}
                      transition={{ duration: 0.2, ease: "easeOut" }}
                      className="inline-flex"
                    >
                      <ChevronDown className="h-4 w-4" />
                    </motion.span>
                  </button>
                </div>
              </div>
              <div
                className={`relative z-10 md-preview md-preview-source text-sm leading-6 text-[var(--text-secondary)] ${
                  !expanded ? "source-preview-scrollbar max-h-28 overflow-y-auto pr-2" : ""
                }`}
              >
                <ReactMarkdown remarkPlugins={markdownRemarkPlugins} rehypePlugins={markdownRehypePlugins}>
                  {expanded ? citation.excerpt : citationContent}
                </ReactMarkdown>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
