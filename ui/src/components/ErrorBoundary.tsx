import { Component, type ErrorInfo, type ReactNode } from "react";

type ErrorBoundaryProps = {
  children: ReactNode;
};

type ErrorBoundaryState = {
  error: Error | null;
};

/**
 * 顶层错误边界：捕获渲染期未处理异常，避免整个应用白屏。
 * 故意不依赖 i18n —— 因为错误可能就发生在 I18nProvider 内部，
 * fallback 必须能在没有任何 context 的情况下独立渲染。
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // 保留到控制台，便于本地诊断 / 日志采集。
    console.error("[ErrorBoundary] 渲染异常被拦截:", error, info.componentStack);
  }

  private handleReload = (): void => {
    window.location.reload();
  };

  render(): ReactNode {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }

    return (
      <div
        style={{
          minHeight: "100vh",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: "2rem",
          background: "#0b0f1a",
          color: "#e2e8f0",
          fontFamily:
            "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, 'PingFang SC', 'Microsoft YaHei', sans-serif"
        }}
      >
        <div style={{ maxWidth: "640px", width: "100%" }}>
          <h1 style={{ fontSize: "1.25rem", fontWeight: 600, marginBottom: "0.75rem" }}>
            界面遇到错误 · Something went wrong
          </h1>
          <p style={{ color: "#94a3b8", marginBottom: "1rem", lineHeight: 1.6 }}>
            应用渲染时出现了未处理的异常，已阻止整页白屏。你的数据未受影响，重新加载通常即可恢复。
            <br />
            The UI hit an unexpected error. Your data is safe — reloading usually recovers.
          </p>
          <pre
            style={{
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              background: "#111827",
              border: "1px solid #1f2937",
              borderRadius: "8px",
              padding: "0.75rem 1rem",
              fontSize: "0.8rem",
              color: "#f87171",
              marginBottom: "1.25rem",
              maxHeight: "240px",
              overflow: "auto"
            }}
          >
            {error.name}: {error.message}
          </pre>
          <button
            type="button"
            onClick={this.handleReload}
            style={{
              background: "#0ea5e9",
              color: "#0b0f1a",
              border: "none",
              borderRadius: "8px",
              padding: "0.6rem 1.25rem",
              fontWeight: 600,
              cursor: "pointer"
            }}
          >
            重新加载 · Reload
          </button>
        </div>
      </div>
    );
  }
}
