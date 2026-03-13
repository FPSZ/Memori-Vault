import { useI18n } from "../../i18n";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type StatusFooterProps = {
  t: TranslateFn;
  stats: {
    documents: number;
    chunks: number;
    nodes: number;
  };
};

export function StatusFooter({ t, stats }: StatusFooterProps) {
  return (
    <footer className="surface-chrome relative z-10 shrink-0 border-t border-[var(--border-subtle)]">
      <div className="mx-auto flex h-8 w-full max-w-5xl items-center justify-between px-6 text-[11px] text-[var(--text-secondary)] md:px-10">
        <span>
          {t("vaultStats", {
            docs: stats.documents,
            chunks: stats.chunks,
            nodes: stats.nodes
          })}
        </span>
        <span className="inline-flex items-center gap-2 text-[var(--accent)]">
          <span className="h-1.5 w-1.5 rounded-full bg-[var(--accent)]" />
          {t("localFirstDaemon")}
        </span>
      </div>
    </footer>
  );
}

