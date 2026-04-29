import { X, FileText } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkBreaks from "remark-breaks";
import rehypeRaw from "rehype-raw";
import rehypeSanitize from "rehype-sanitize";
import rehypeHighlight from "rehype-highlight";
import { useI18n } from "../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type FilePreviewProps = {
  t: TranslateFn;
  filePath: string;
  content: string;
  format?: string;
  onClose: () => void;
};

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const REHYPE_PLUGINS = [rehypeRaw, rehypeSanitize, rehypeHighlight];

function renderDocumentText(content: string) {
  const blocks = content
    .split(/\n{2,}/)
    .map((block) => block.trim())
    .filter(Boolean);
  return blocks.map((block, index) => {
    const lines = block.split('\n').map((line) => line.trim()).filter(Boolean);
    const isTableLike = lines.length > 1 && lines.every((line) => line.includes(" | "));
    if (isTableLike) {
      return (
        <div key={index} className="overflow-x-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface-2)]/60">
          <table className="w-full border-collapse text-left text-sm">
            <tbody>
              {lines.map((line, rowIndex) => (
                <tr key={rowIndex} className="border-b border-[var(--border-subtle)] last:border-0">
                  {line.split(/\s+\|\s+/).map((cell, cellIndex) => (
                    <td key={cellIndex} className="px-3 py-2 align-top text-[var(--text-primary)]">
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
    return (
      <p key={index} className="whitespace-pre-wrap text-sm leading-7 text-[var(--text-primary)]">
        {block}
      </p>
    );
  });
}

export function FilePreview({ t, filePath, content, format, onClose }: FilePreviewProps) {
  const fileName = filePath.split(/[/\\]/).pop() || filePath;
  const isMarkdown = (format ?? "").toLowerCase() === "markdown" || fileName.toLowerCase().endsWith(".md");
  const isDocument = (format ?? "").toLowerCase() === "document";

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-4 py-2.5">
        <div className="flex min-w-0 items-center gap-2">
          <FileText className="h-4 w-4 shrink-0 text-[var(--accent)]" />
          <span className="min-w-0 truncate text-sm font-medium text-[var(--text-primary)]">
            {fileName}
          </span>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="inline-flex h-7 w-7 items-center justify-center rounded-md text-[var(--text-muted)] transition hover:bg-[var(--bg-surface-2)] hover:text-[var(--text-primary)]"
          title={t("back")}
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto">
        {isMarkdown ? (
          <div className="md-preview px-6 py-5 text-[var(--text-primary)]">
            <ReactMarkdown remarkPlugins={REMARK_PLUGINS} rehypePlugins={REHYPE_PLUGINS}>
              {content}
            </ReactMarkdown>
          </div>
        ) : isDocument ? (
          <div className="document-preview space-y-4 px-6 py-5">
            {renderDocumentText(content)}
          </div>
        ) : (
          <pre className="m-0 whitespace-pre-wrap break-words p-4 text-sm leading-relaxed text-[var(--text-primary)] font-mono">
            {content}
          </pre>
        )}
      </div>
    </div>
  );
}
