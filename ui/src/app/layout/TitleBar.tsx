import { motion } from "framer-motion";
import { Minus, Moon, Settings as SettingsIcon, Square, Sun, X } from "lucide-react";
import { useI18n } from "../../i18n";
import type { ThemeMode } from "../../components/settings/types";

type TranslateFn = ReturnType<typeof useI18n>["t"];

type TitleBarProps = {
  t: TranslateFn;
  headerWatchRoot: string;
  headerSelectedCount: string;
  themeMode: ThemeMode;
  isMaximized: boolean;
  onToggleThemeMode: () => void;
  onToggleSettings: () => void;
  onMinimize: () => Promise<void>;
  onToggleMaximize: () => Promise<void>;
  onClose: () => Promise<void>;
};

export function TitleBar({
  t,
  headerWatchRoot,
  headerSelectedCount,
  themeMode,
  isMaximized,
  onToggleThemeMode,
  onToggleSettings,
  onMinimize,
  onToggleMaximize,
  onClose
}: TitleBarProps) {
  return (
    <header
      data-tauri-drag-region=""
      className="surface-chrome relative z-50 flex h-9 shrink-0 items-center pl-2 pr-2 select-none [app-region:drag] [-webkit-app-region:drag]"
    >
      <div
        data-tauri-drag-region=""
        className="pointer-events-none absolute inset-0 flex items-center justify-center px-44"
      >
        <div className="inline-flex min-w-0 max-w-[62vw] items-center gap-2 px-1 text-[10px] text-[var(--text-secondary)]">
          <span className="shrink-0 uppercase tracking-[0.08em]">{t("watchRoot")}</span>
          <span className="min-w-0 truncate text-[var(--text-primary)]">{headerWatchRoot}</span>
          <span className="shrink-0 text-[var(--text-muted)]">|</span>
          <span className="shrink-0">{headerSelectedCount}</span>
        </div>
      </div>
      <div data-tauri-drag-region="" className="h-full flex-1 cursor-move" />
      <div className="flex items-center gap-1.5 [app-region:no-drag] [-webkit-app-region:no-drag]">
        <motion.button
          type="button"
          onClick={onToggleThemeMode}
          className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
          aria-label={t("themeToggle")}
          title={t("themeToggle")}
          whileTap={{ scale: 0.9 }}
          animate={{ rotate: themeMode === "dark" ? 0 : 180 }}
          transition={{ type: "spring", damping: 16, stiffness: 180 }}
        >
          {themeMode === "dark" ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
        </motion.button>
        <button
          type="button"
          onClick={onToggleSettings}
          className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--accent)]"
          aria-label={t("settings")}
          title={t("settings")}
        >
          <SettingsIcon className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => void onMinimize()}
          className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
          aria-label={t("windowMinimize")}
          title={t("windowMinimize")}
        >
          <Minus className="h-4 w-4" />
        </button>
        <button
          type="button"
          onClick={() => void onToggleMaximize()}
          className="inline-flex items-center justify-center p-1 text-[var(--text-secondary)] transition hover:text-[var(--text-primary)]"
          aria-label={isMaximized ? t("windowRestore") : t("windowMaximize")}
          title={isMaximized ? t("windowRestore") : t("windowMaximize")}
        >
          <Square className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={() => void onClose()}
          className="inline-flex items-center justify-center p-1 text-[var(--danger)] transition hover:text-red-400"
          aria-label={t("windowClose")}
          title={t("windowClose")}
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </header>
  );
}

