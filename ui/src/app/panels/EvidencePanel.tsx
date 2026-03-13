import { ChevronDown, ChevronUp, FolderOpen } from "lucide-react";
import ReactMarkdown from "react-markdown";
import {
  buildCollapsedMarkdownPreview,
  formatDocumentReason,
  formatEvidenceReason,
  isMarkdownFile
} from "../formatters";
import type { Translate, VisibleEvidenceGroup } from "../types";

type EvidencePanelProps = {
  visibleEvidenceGroups: VisibleEvidenceGroup[];
  expandedSourceKeys: Set<string>;
  onToggleSourceExpanded: (key: string) => void;
  onOpenSourceLocation: (path: string) => void;
  markdownRemarkPlugins: any[];
  markdownRehypePlugins: any[];
  t: Translate;
};

export function EvidencePanel({
  visibleEvidenceGroups,
  expandedSourceKeys,
  onToggleSourceExpanded,
  onOpenSourceLocation,
  markdownRemarkPlugins,
  markdownRehypePlugins,
  t
}: EvidencePanelProps) {
  if (visibleEvidenceGroups.length === 0) {
    return null;
  }

  return (
    <section className="mt-8">
      <div className="mb-3 text-[11px] tracking-[0.16em] text-[var(--text-secondary)] uppercase">{t("evidenceTitle")}</div>

      <div className="grid grid-cols-1 items-stretch gap-3 md:grid-cols-2">
        {visibleEvidenceGroups.map((source) => {
          const sourceKey = source.evidence_key;
          const expanded = expandedSourceKeys.has(sourceKey);
          const markdownPreview = isMarkdownFile(source.file_path);
          const markdownContent = expanded ? source.content : buildCollapsedMarkdownPreview(source.content, 7);

          return (
            <div
              key={sourceKey}
              className={`surface-lite relative flex h-full flex-col rounded-xl px-4 py-3 ${
                expanded ? "md:col-span-2" : ""
              }`}
            >
              <div className="min-w-0 flex-1 pr-12">
                <div className="flex flex-wrap items-center gap-2 text-[11px]">
                  {source.document_reasons.map((reason) => (
                    <span
                      key={`doc-${sourceKey}-${reason}`}
                      className="rounded-full bg-[var(--bg-surface-2)] px-2 py-0.5 text-[var(--text-secondary)]"
                    >
                      {formatDocumentReason(reason, t)}
                    </span>
                  ))}
                  {source.reasons.map((reason) => (
                    <span
                      key={`reason-${sourceKey}-${reason}`}
                      className="rounded-full bg-[var(--accent-soft)] px-2 py-0.5 text-[var(--accent)]"
                    >
                      {formatEvidenceReason(reason, t)}
                    </span>
                  ))}
                  <span className="text-[var(--text-secondary)]">
                    {t("documentRankLabel", { count: source.document_rank })}
                  </span>
                  <span className="text-[var(--text-secondary)]">
                    {t("chunkRankLabel", { count: source.top_chunk_rank })}
                  </span>
                  <span className="text-[var(--text-muted)]">{t("evidenceFragments", { count: source.fragment_count })}</span>
                </div>
                <div className="mt-2 truncate font-mono text-xs text-[var(--text-secondary)]" title={source.file_path}>
                  {source.relative_path || source.file_path}
                </div>
                <div className="mt-1 text-[11px] text-[var(--text-muted)]">
                  {source.block_kinds.join(" / ")}
                  {source.heading_paths.length > 0 ? ` · ${source.heading_paths.join(" · ")}` : ""}
                </div>
                {markdownPreview ? (
                  <div
                    className={`md-preview md-preview-source mt-2 text-sm leading-6 text-[var(--text-secondary)] ${
                      !expanded ? "source-preview-scrollbar max-h-24 overflow-y-auto pr-2" : ""
                    }`}
                  >
                    <ReactMarkdown remarkPlugins={markdownRemarkPlugins} rehypePlugins={markdownRehypePlugins}>
                      {expanded ? source.content : markdownContent}
                    </ReactMarkdown>
                  </div>
                ) : (
                  <div
                    className={`mt-2 whitespace-pre-wrap break-words font-mono text-[13px] leading-6 text-[var(--text-muted)] ${
                      !expanded ? "source-preview-scrollbar max-h-24 overflow-y-auto pr-2" : ""
                    }`}
                  >
                    {expanded ? source.content : markdownContent}
                  </div>
                )}
              </div>

              <button
                type="button"
                onClick={() => onOpenSourceLocation(source.file_path)}
                className="absolute right-8 top-3 p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                aria-label={t("openSourceLocation")}
                title={t("openSourceLocation")}
              >
                <FolderOpen className="h-4 w-4" />
              </button>

              <button
                type="button"
                onClick={() => onToggleSourceExpanded(sourceKey)}
                className="absolute right-3 top-3 p-0 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
                aria-label={expanded ? t("collapseSource") : t("expandSource")}
                title={expanded ? t("collapseSource") : t("expandSource")}
              >
                {expanded ? <ChevronUp className="h-4 w-4" /> : <ChevronDown className="h-4 w-4" />}
              </button>
            </div>
          );
        })}
      </div>
    </section>
  );
}
