import { isLongCitationExcerpt } from "./formatters";
import type { CitationItem, EvidenceItem, VisibleCitation, VisibleEvidenceGroup } from "./types";

export function normalizeEvidenceContent(content: string): string {
  return content
    .replace(/\r\n/g, "\n")
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export function mergeEvidenceFragments(items: EvidenceItem[]): string {
  const uniqueFragments: string[] = [];

  for (const item of items) {
    const normalized = normalizeEvidenceContent(item.content);
    if (!normalized) {
      continue;
    }

    const duplicateIndex = uniqueFragments.findIndex((existing) => {
      if (existing === normalized) {
        return true;
      }
      if (existing.includes(normalized)) {
        return true;
      }
      if (normalized.includes(existing)) {
        return true;
      }
      return false;
    });

    if (duplicateIndex >= 0) {
      if (normalized.length > uniqueFragments[duplicateIndex].length) {
        uniqueFragments[duplicateIndex] = normalized;
      }
      continue;
    }

    uniqueFragments.push(normalized);
  }

  return uniqueFragments.join("\n\n---\n\n");
}

export function buildVisibleEvidenceGroups(visibleEvidence: EvidenceItem[]): VisibleEvidenceGroup[] {
  const groups = new Map<string, EvidenceItem[]>();
  for (const source of visibleEvidence) {
    const key = source.file_path.toLowerCase();
    const bucket = groups.get(key);
    if (bucket) {
      bucket.push(source);
    } else {
      groups.set(key, [source]);
    }
  }

  return Array.from(groups.values())
    .map((items) => {
      const sortedItems = [...items].sort((a, b) => a.chunk_rank - b.chunk_rank);
      const first = sortedItems[0];
      const headingPaths = Array.from(
        new Set(
          sortedItems
            .map((item) => item.heading_path.join(" > ").trim())
            .filter((value) => value.length > 0)
        )
      );
      const blockKinds = Array.from(new Set(sortedItems.map((item) => item.block_kind.trim()).filter(Boolean)));
      const documentReasons = Array.from(new Set(sortedItems.map((item) => item.document_reason)));
      const reasons = Array.from(new Set(sortedItems.map((item) => item.reason)));
      const chunkRanks = sortedItems.map((item) => item.chunk_rank);
      return {
        evidence_key: `${first.file_path.toLowerCase()}::${chunkRanks.join(",")}`,
        file_path: first.file_path,
        relative_path: first.relative_path,
        heading_paths: headingPaths,
        block_kinds: blockKinds,
        document_reasons: documentReasons,
        reasons,
        document_rank: Math.min(...sortedItems.map((item) => item.document_rank)),
        top_chunk_rank: Math.min(...chunkRanks),
        chunk_ranks: chunkRanks,
        content: mergeEvidenceFragments(sortedItems),
        fragment_count: sortedItems.length
      } satisfies VisibleEvidenceGroup;
    })
    .sort((a, b) => {
      if (a.document_rank !== b.document_rank) {
        return a.document_rank - b.document_rank;
      }
      return a.top_chunk_rank - b.top_chunk_rank;
    });
}

export function buildVisibleCitations(citations: CitationItem[], retrieveTopK: number): VisibleCitation[] {
  const grouped = new Map<string, VisibleCitation>();
  for (const citation of citations) {
    const excerpt = citation.excerpt.trim();
    const citationKey = `${citation.file_path.toLowerCase()}::${excerpt}`;
    const existing = grouped.get(citationKey);
    if (existing) {
      existing.duplicate_count += 1;
      continue;
    }
    grouped.set(citationKey, {
      ...citation,
      citation_key: citationKey,
      duplicate_count: 1,
      is_long_excerpt: isLongCitationExcerpt(citation.excerpt)
    });
  }

  return Array.from(grouped.values())
    .map((citation, index) => ({
      ...citation,
      index: index + 1
    }))
    .slice(0, retrieveTopK);
}
