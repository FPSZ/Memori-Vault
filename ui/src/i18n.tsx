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
    topK: "Top 检索条数",
    topKDesc: "控制每次问答参考来源数量。",
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
    themeModeDark: "深色",
    themeModeLight: "浅色",
    themeToggleDesc: "切换应用主题外观（深色/浅色）。",
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
    collapseSource: "收起来源内容",
    scopeAll: "全部范围",
    scopeSelectTitle: "搜索范围",
    scopeSelectedCount: "{count} 个文件和目录",
    scopeLoading: "加载范围中...",
    scopeNoItems: "当前目录下没有可选子文件夹或文本文件",
    modelProvider: "模型来源",
    providerOllama: "本地 Ollama",
    providerOpenAI: "远程 API",
    modelEndpoint: "服务地址",
    modelApiKey: "API Key",
    chatModel: "对话模型",
    graphModel: "图谱模型",
    embedModel: "向量模型",
    modelLocalRoot: "本地模型目录",
    modelLocalRootPick: "选择目录",
    modelLocalRootClear: "清空",
    modelMergedCandidates: "可选模型",
    modelFromFolder: "目录扫描",
    modelFromService: "服务列表",
    modelNoCandidates: "暂无可用模型，可先点击“刷新模型列表”。",
    modelUseCustom: "自定义",
    modelCustomPlaceholder: "输入模型名",
    modelStatusTitle: "连接与可用性",
    modelStatusReachable: "连接成功",
    modelStatusUnreachable: "连接失败",
    modelStatusMissing: "缺失角色: {roles}",
    modelStatusReady: "三种角色模型均可用",
    modelStatusProvider: "当前校验来源: {provider}",
    modelActionProbeOk: "连接测试完成。",
    modelActionRefreshOk: "模型列表已刷新。",
    modelActionSaveOk: "模型配置已保存并热重载。",
    modelActionPullOk: "缺失模型拉取完成。",
    actionConnecting: "连接中...",
    actionConnected: "连接成功",
    actionConnectFailed: "连接失败",
    actionRefreshing: "刷新中...",
    actionRefreshed: "已刷新",
    actionRefreshFailed: "刷新失败",
    actionSaving: "保存中...",
    actionSaved: "已保存",
    actionSaveFailed: "保存失败",
    actionPulling: "拉取中...",
    actionPulled: "已拉取",
    actionPullFailed: "拉取失败",
    saveModels: "保存配置",
    testConnection: "测试连接",
    refreshModels: "刷新模型列表",
    pullMissingModels: "一键拉取缺失模型",
    modelSetupNeeded: "模型配置未完成，请先完成向导",
    setupWizard: "模型启动向导",
    nextStep: "下一步",
    previousStep: "上一步",
    finishSetup: "完成设置",
    closeWizard: "稍后设置"
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
    topK: "Top Retrieval Count",
    topKDesc: "Controls how many context sources each answer uses.",
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
    themeModeDark: "Dark",
    themeModeLight: "Light",
    themeToggleDesc: "Switch the app theme appearance (dark/light).",
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
    collapseSource: "Collapse source",
    scopeAll: "All scopes",
    scopeSelectTitle: "Search scope",
    scopeSelectedCount: "{count} files/folders",
    scopeLoading: "Loading scopes...",
    scopeNoItems: "No subfolders or text files available under current watch root",
    modelProvider: "Model Provider",
    providerOllama: "Local Ollama",
    providerOpenAI: "Remote API",
    modelEndpoint: "Endpoint",
    modelApiKey: "API Key",
    chatModel: "Chat Model",
    graphModel: "Graph Model",
    embedModel: "Embed Model",
    modelLocalRoot: "Local Models Folder",
    modelLocalRootPick: "Pick Folder",
    modelLocalRootClear: "Clear",
    modelMergedCandidates: "Available Models",
    modelFromFolder: "Folder Scan",
    modelFromService: "Service List",
    modelNoCandidates: "No models found yet. Try refresh first.",
    modelUseCustom: "Custom",
    modelCustomPlaceholder: "Type model name",
    modelStatusTitle: "Connectivity & Availability",
    modelStatusReachable: "Reachable",
    modelStatusUnreachable: "Unreachable",
    modelStatusMissing: "Missing roles: {roles}",
    modelStatusReady: "All role models are available",
    modelStatusProvider: "Checked provider: {provider}",
    modelActionProbeOk: "Connection test completed.",
    modelActionRefreshOk: "Model list refreshed.",
    modelActionSaveOk: "Model settings saved and hot-reloaded.",
    modelActionPullOk: "Missing model pull completed.",
    actionConnecting: "Connecting...",
    actionConnected: "Connected",
    actionConnectFailed: "Connect failed",
    actionRefreshing: "Refreshing...",
    actionRefreshed: "Refreshed",
    actionRefreshFailed: "Refresh failed",
    actionSaving: "Saving...",
    actionSaved: "Saved",
    actionSaveFailed: "Save failed",
    actionPulling: "Pulling...",
    actionPulled: "Pulled",
    actionPullFailed: "Pull failed",
    saveModels: "Save Settings",
    testConnection: "Test Connection",
    refreshModels: "Refresh Model List",
    pullMissingModels: "Pull Missing Models",
    modelSetupNeeded: "Model setup is incomplete. Please run onboarding.",
    setupWizard: "Model Onboarding",
    nextStep: "Next",
    previousStep: "Back",
    finishSetup: "Finish",
    closeWizard: "Later"
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
