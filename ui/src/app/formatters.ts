import type { AskStatus, EvidenceItem, Translate } from "./types";

export function formatElapsed(ms: number): string {
  const safe = Math.max(0, ms);
  if (safe < 60_000) {
    return `${(safe / 1000).toFixed(1)}s`;
  }
  const totalSeconds = Math.round(safe / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown|mdx)$/i.test(path.trim());
}

export function buildCollapsedMarkdownPreview(content: string, maxLines = 8): string {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return normalized;
  }

  const fenceMatch = normalized.match(/```[\w-]*\n[\s\S]*?\n```/);
  if (fenceMatch) {
    const fenceIndex = fenceMatch.index ?? 0;
    const before = normalized.slice(0, fenceIndex).trim();
    const beforeLines = before ? before.split("\n").slice(-Math.min(maxLines, 6)).join("\n") : "";
    return beforeLines ? `${beforeLines}\n\n${fenceMatch[0]}` : fenceMatch[0];
  }

  return normalized.split("\n").slice(0, maxLines).join("\n");
}

export function formatMetricDuration(ms: number): string {
  return `${Math.round(Math.max(0, ms))}ms`;
}

export function formatQueryFlag(flag: string, t: Translate): string {
  switch (flag) {
    case "cjk":
      return t("flagCjk");
    case "ascii_identifier":
      return t("flagAsciiIdentifier");
    case "path_like":
      return t("flagPathLike");
    case "lookup_like":
      return t("flagLookupLike");
    default:
      break;
  }

  if (flag.startsWith("token_count:")) {
    return t("flagTokenCount", { count: flag.slice("token_count:".length) });
  }
  if (flag.startsWith("identifier_terms:")) {
    return t("flagIdentifierTerms", { count: flag.slice("identifier_terms:".length) });
  }
  if (flag.startsWith("filename_terms:")) {
    return t("flagFilenameTerms", { count: flag.slice("filename_terms:".length) });
  }
  if (flag.startsWith("query_family:")) {
    const family = flag.slice("query_family:".length);
    const value =
      family === "docs_explanatory"
        ? t("queryFamilyDocsExplanatory")
        : family === "docs_api_lookup"
          ? t("queryFamilyDocsApiLookup")
          : family === "implementation_lookup"
            ? t("queryFamilyImplementationLookup")
            : family;
    return t("flagQueryFamily", { value });
  }
  if (flag.startsWith("intent:")) {
    const intent = flag.slice("intent:".length);
    const value =
      intent === "repo_lookup"
        ? t("intentRepoLookup")
        : intent === "repo_question"
          ? t("intentRepoQuestion")
          : intent === "external_fact"
            ? t("intentExternalFact")
            : intent === "secret_request"
              ? t("intentSecretRequest")
              : intent === "missing_file_lookup"
                ? t("intentMissingFileLookup")
                : intent;
    return t("flagIntent", { value });
  }

  return flag;
}

export function isLongCitationExcerpt(content: string): boolean {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return false;
  }
  return normalized.length > 420 || normalized.split("\n").length > 10;
}

export function formatEvidenceReason(reason: EvidenceItem["reason"], t: Translate): string {
  switch (reason) {
    case "both":
      return t("evidenceReasonBoth");
    case "lexical":
      return t("evidenceReasonLexical");
    case "dense":
      return t("evidenceReasonDense");
    default:
      return reason;
  }
}

export function formatDocumentReason(reason: EvidenceItem["document_reason"], t: Translate): string {
  switch (reason) {
    case "both":
      return t("documentReasonBoth");
    case "lexical":
      return t("documentReasonLexical");
    case "lexical_strict":
      return t("documentReasonLexicalStrict");
    case "lexical_broad":
      return t("documentReasonLexicalBroad");
    case "mixed":
      return t("documentReasonMixed");
    case "exact_path":
      return t("documentReasonExactPath");
    case "exact_symbol":
      return t("documentReasonExactSymbol");
    case "docs_phrase":
      return t("documentReasonDocsPhrase");
    case "filename":
      return t("documentReasonFilename");
    case "scope":
      return t("documentReasonScope");
    default:
      return reason;
  }
}

export function statusToneClasses(status: AskStatus): string {
  switch (status) {
    case "answered":
      return "border-[color-mix(in_srgb,var(--accent)_34%,transparent)] bg-[color-mix(in_srgb,var(--accent)_12%,var(--bg-surface-1)_88%)] text-[color-mix(in_srgb,var(--accent)_76%,var(--text-primary)_24%)]";
    case "model_failed_with_evidence":
      return "border-amber-500/30 bg-amber-500/10 text-amber-200";
    default:
      return "border-[var(--border-strong)] bg-[var(--bg-surface-2)] text-[var(--text-secondary)]";
  }
}

export function normalizeScopeKey(relativePath: string, fallback: string): string {
  const normalized = relativePath.replaceAll("\\", "/").replace(/^\/+|\/+$/g, "");
  if (normalized) {
    return normalized;
  }
  return fallback.replaceAll("\\", "/");
}
