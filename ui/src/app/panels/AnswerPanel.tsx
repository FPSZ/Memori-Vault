import { Atom, Sparkles } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { formatElapsed, statusToneClasses } from "../formatters";
import type { AskResponseStructured, Translate } from "../types";

type AnswerPanelProps = {
  answerResponse: AskResponseStructured;
  lastSearchDurationMs: number | null;
  markdownRemarkPlugins: any[];
  markdownRehypePlugins: any[];
  t: Translate;
};

export function AnswerPanel({
  answerResponse,
  lastSearchDurationMs,
  markdownRemarkPlugins,
  markdownRehypePlugins,
  t
}: AnswerPanelProps) {
  return (
    <>
      <div className={`mb-5 rounded-xl border px-4 py-3 text-sm ${statusToneClasses(answerResponse.status)}`}>
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <Sparkles className="h-4 w-4" />
            <span className="font-semibold">
              {answerResponse.status === "answered"
                ? t("answerStatusAnswered")
                : answerResponse.status === "model_failed_with_evidence"
                  ? t("answerStatusModelFailed")
                  : t("answerStatusInsufficient")}
            </span>
          </div>
          {lastSearchDurationMs !== null ? (
            <span className="text-[11px] text-[var(--text-secondary)]">
              {t("elapsedTime", { time: formatElapsed(lastSearchDurationMs) })}
            </span>
          ) : null}
        </div>
        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-[var(--text-secondary)]">
          <span>{t("docCandidateCount", { count: answerResponse.metrics.doc_candidate_count })}</span>
          <span>{t("chunkCandidateCount", { count: answerResponse.metrics.chunk_candidate_count })}</span>
          <span>{t("finalEvidenceCount", { count: answerResponse.metrics.final_evidence_count })}</span>
        </div>
      </div>

      {answerResponse.answer.trim().length > 0 && (
        <div className="mb-6 border-l-2 border-[var(--accent)] pl-4">
          <div className="mb-3 flex items-center gap-2">
            <Atom className="h-4 w-4 text-[var(--accent)]" />
            <span className="text-xs font-bold tracking-widest text-[var(--accent)]">{t("synthesis")}</span>
          </div>
          <div className="md-preview mt-1 break-words font-sans text-lg leading-relaxed text-[var(--text-primary)]">
            <ReactMarkdown remarkPlugins={markdownRemarkPlugins} rehypePlugins={markdownRehypePlugins}>
              {answerResponse.answer}
            </ReactMarkdown>
          </div>
        </div>
      )}
    </>
  );
}
