import { createContext, ReactNode, useContext, useMemo, useState } from "react";

export type Language = "zh-CN" | "en-US";
export type AiLanguage = "zh-CN" | "en-US";

const UI_LANG_STORAGE_KEY = "memori-ui-language";

const MESSAGES = {
  "zh-CN": {
    settings: "设置",
    back: "返回",
    basic: "基础",
    engine: "引擎",
    models: "模型",
    advanced: "高级",
    personalization: "个性化",
    settingsSearchPlaceholder: "搜索设置...",
    noSettingsMatch: "未找到匹配设置项",
    autoSyncDaemon: "自动同步守护",
    autoSyncDaemonDesc: "启用后台文件监听。",
    graphRagInfer: "图谱推理",
    graphRagInferDesc: "在答案合成中启用图谱上下文推理。",
    graphExtractorModel: "图谱抽取模型",
    graphExtractorModelDesc:
      "用于实体与关系抽取。推荐 qwen2.5:7b 以获得更稳定的 JSON 输出。",
    uiLanguage: "界面语言",
    aiReplyLanguage: "AI 回答语言",
    watchRoot: "读取文件夹",
    watchRootPick: "选择文件夹",
    watchRootPicking: "选择中...",
    watchRootRestartHint: "目录切换后会立即重建监听与索引状态。",
    fontPreset: "字体风格",
    fontPresetSystem: "系统优雅",
    fontPresetNeo: "现代几何",
    fontPresetMono: "技术混排",
    fontSize: "字体大小",
    fontSizeS: "小",
    fontSizeM: "中",
    fontSizeL: "大",
    accentColor: "主题色",
    accentBlue: "极光蓝",
    accentGreen: "霓虹绿",
    accentAmber: "琥珀橙",
    themeToggle: "主题形态",
    themeModeA: "夜色 A",
    themeModeB: "夜色 B",
    themeToggleDesc: "仅切换深色风格状态，不启用浅色主题。",
    askPlaceholder: "向你的 Vault 提问...",
    loading: "正在检索并合成答案...",
    synthesis: "SYNTHESIS",
    contextSources: "CONTEXT SOURCES",
    semanticRelevance: "语义相关度",
    localFirstDaemon: "本地优先守护进程",
    vaultStats: "Vault: {docs} 文档 / {chunks} 分块 / {nodes} 节点",
    settingsTitle: "设置中心",
    openSourceLocation: "打开文件位置",
    expandSource: "展开来源内容",
    collapseSource: "收起来源内容"
  },
  "en-US": {
    settings: "Settings",
    back: "Back",
    basic: "Basic",
    engine: "Engine",
    models: "Models",
    advanced: "Advanced",
    personalization: "Personalization",
    settingsSearchPlaceholder: "Search settings...",
    noSettingsMatch: "No matching settings",
    autoSyncDaemon: "Auto-Sync Daemon",
    autoSyncDaemonDesc: "Enable background file watching.",
    graphRagInfer: "Graph-RAG Infer",
    graphRagInferDesc: "Enable graph context inference in answer synthesis.",
    graphExtractorModel: "Graph Extractor Model",
    graphExtractorModelDesc:
      "Used for entity and relation extraction. qwen2.5:7b is recommended for stable JSON output.",
    uiLanguage: "UI Language",
    aiReplyLanguage: "AI Reply Language",
    watchRoot: "Watch Folder",
    watchRootPick: "Pick Folder",
    watchRootPicking: "Picking...",
    watchRootRestartHint: "Changing folder will immediately restart watch and indexing state.",
    fontPreset: "Font Preset",
    fontPresetSystem: "System Elegant",
    fontPresetNeo: "Neo Geometric",
    fontPresetMono: "Tech Mixed",
    fontSize: "Font Size",
    fontSizeS: "Small",
    fontSizeM: "Medium",
    fontSizeL: "Large",
    accentColor: "Accent Color",
    accentBlue: "Aurora Blue",
    accentGreen: "Neon Green",
    accentAmber: "Amber Orange",
    themeToggle: "Theme Mode",
    themeModeA: "Night A",
    themeModeB: "Night B",
    themeToggleDesc: "Placeholder switch in dark mode only; no light theme yet.",
    askPlaceholder: "Ask your vault...",
    loading: "Retrieving and synthesizing...",
    synthesis: "SYNTHESIS",
    contextSources: "CONTEXT SOURCES",
    semanticRelevance: "Semantic relevance",
    localFirstDaemon: "Local-First Daemon",
    vaultStats: "Vault: {docs} Docs / {chunks} Chunks / {nodes} Nodes",
    settingsTitle: "Settings",
    openSourceLocation: "Open file location",
    expandSource: "Expand source",
    collapseSource: "Collapse source"
  }
} as const;

type MessageKey = keyof (typeof MESSAGES)["zh-CN"];

type I18nContextValue = {
  lang: Language;
  setLang: (lang: Language) => void;
  t: (key: MessageKey, vars?: Record<string, string | number>) => string;
};

const I18nContext = createContext<I18nContextValue | null>(null);

function detectDefaultLanguage(): Language {
  const navLang = typeof navigator !== "undefined" ? navigator.language.toLowerCase() : "";
  return navLang.startsWith("zh") ? "zh-CN" : "en-US";
}

function resolveInitialLanguage(): Language {
  if (typeof window === "undefined") {
    return "en-US";
  }

  const saved = window.localStorage.getItem(UI_LANG_STORAGE_KEY);
  if (saved === "zh-CN" || saved === "en-US") {
    return saved;
  }
  return detectDefaultLanguage();
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Language>(() => resolveInitialLanguage());

  const setLang = (next: Language) => {
    setLangState(next);
    if (typeof window !== "undefined") {
      window.localStorage.setItem(UI_LANG_STORAGE_KEY, next);
    }
  };

  const value = useMemo<I18nContextValue>(() => {
    const t = (key: MessageKey, vars?: Record<string, string | number>) => {
      let text: string = MESSAGES[lang][key];
      if (!vars) {
        return text;
      }

      for (const [k, v] of Object.entries(vars)) {
        text = text.replaceAll(`{${k}}`, String(v));
      }
      return text;
    };

    return { lang, setLang, t };
  }, [lang]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error("useI18n must be used inside <I18nProvider />");
  }
  return ctx;
}
