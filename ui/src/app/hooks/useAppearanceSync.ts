import { useEffect, useLayoutEffect } from "react";
import type { Language } from "../../i18n";
import type { FontPreset, FontScale, ThemeMode } from "../../components/settings/types";

type UseAppearanceSyncOptions = {
  aiLang: Language;
  themeMode: ThemeMode;
  fontPreset: FontPreset;
  fontScale: FontScale;
  retrieveTopK: number;
  aiLangStorageKey: string;
  themeStorageKey: string;
  legacyThemeModeStorageKey: string;
  fontPresetStorageKey: string;
  fontScaleStorageKey: string;
  retrieveTopKStorageKey: string;
};

export function useAppearanceSync({
  aiLang,
  themeMode,
  fontPreset,
  fontScale,
  retrieveTopK,
  aiLangStorageKey,
  themeStorageKey,
  legacyThemeModeStorageKey,
  fontPresetStorageKey,
  fontScaleStorageKey,
  retrieveTopKStorageKey
}: UseAppearanceSyncOptions) {
  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(aiLangStorageKey, aiLang);
    }
  }, [aiLang, aiLangStorageKey]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(themeStorageKey, themeMode);
    window.localStorage.removeItem(legacyThemeModeStorageKey);
  }, [legacyThemeModeStorageKey, themeMode, themeStorageKey]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(fontPresetStorageKey, fontPreset);
  }, [fontPreset, fontPresetStorageKey]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(fontScaleStorageKey, fontScale);
  }, [fontScale, fontScaleStorageKey]);

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    window.localStorage.setItem(retrieveTopKStorageKey, String(retrieveTopK));
  }, [retrieveTopK, retrieveTopKStorageKey]);

  useLayoutEffect(() => {
    const root = document.documentElement;
    const fontPresetMap: Record<FontPreset, { regular: string; mono: string }> = {
      system: {
        regular:
          '"PingFang SC","Microsoft YaHei","Segoe UI",Inter,"SF Pro Text","Noto Sans",sans-serif',
        mono:
          '"Cascadia Mono","JetBrains Mono","Maple Mono","IBM Plex Mono","Consolas","SFMono-Regular","Noto Sans Mono",monospace'
      },
      neo: {
        regular:
          '"HarmonyOS Sans SC","Segoe UI Variable","Segoe UI",Inter,"SF Pro Display","Noto Sans",sans-serif',
        mono:
          '"Berkeley Mono","JetBrains Mono","Cascadia Mono","IBM Plex Mono","Consolas","Noto Sans Mono",monospace'
      },
      mono: {
        regular:
          '"IBM Plex Sans","PingFang SC","Microsoft YaHei","Segoe UI",Inter,sans-serif',
        mono:
          '"IBM Plex Mono","Sarasa Mono SC","JetBrains Mono","Cascadia Mono","Consolas","Noto Sans Mono",monospace'
      }
    };
    const fontScaleMap: Record<FontScale, string> = {
      s: "14px",
      m: "16px",
      l: "18px"
    };

    root.style.setProperty("--app-font-family", fontPresetMap[fontPreset].regular);
    root.style.setProperty("--app-font-family-mono", fontPresetMap[fontPreset].mono);
    root.style.setProperty("--app-font-size", fontScaleMap[fontScale]);
    root.style.fontSize = fontScaleMap[fontScale];
    root.setAttribute("data-theme", themeMode);
    root.style.colorScheme = themeMode;
  }, [fontPreset, fontScale, themeMode]);
}
