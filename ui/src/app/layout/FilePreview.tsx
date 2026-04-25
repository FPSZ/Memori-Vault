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
  onClose: () => void;
};

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const REHYPE_PLUGINS = [rehypeRaw, rehypeSanitize, rehypeHighlight];

export function FilePreview({ t, filePath, content, onClose }: FilePreviewProps) {
  const fileName = filePath.split(/[/\\]/).pop() || filePath;
  const isMarkdown = fileName.toLowerCase().endsWith(".md");

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
        ) : (
          <pre className="m-0 whitespace-pre-wrap break-words p-4 text-sm leading-relaxed text-[var(--text-primary)] font-mono">
            {content}
          </pre>
        )}
      </div>
    </div>
  );
}
