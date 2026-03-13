import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

type UseWindowControlsOptions = {
  toUiErrorMessage: (error: unknown) => string;
  onError: (message: string) => void;
};

function isTauriHostAvailable(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  const w = window as Window & {
    __TAURI__?: unknown;
    __TAURI_INTERNALS__?: unknown;
  };

  return Boolean(w.__TAURI__ || w.__TAURI_INTERNALS__);
}

export function useWindowControls({ toUiErrorMessage, onError }: UseWindowControlsOptions) {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    let mounted = true;

    const syncMaximizeState = async () => {
      if (!isTauriHostAvailable()) {
        return;
      }

      try {
        const maximized = await getCurrentWindow().isMaximized();
        if (mounted) {
          setIsMaximized(maximized);
        }
      } catch {
        // Best-effort UI sync only.
      }
    };

    void syncMaximizeState();

    return () => {
      mounted = false;
    };
  }, []);

  const onMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (err) {
      onError(toUiErrorMessage(err));
    }
  };

  const onToggleMaximize = async () => {
    try {
      const win = getCurrentWindow();
      await win.toggleMaximize();
      const maximized = await win.isMaximized();
      setIsMaximized(maximized);
    } catch (err) {
      onError(toUiErrorMessage(err));
    }
  };

  const onClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (err) {
      onError(toUiErrorMessage(err));
    }
  };

  return {
    isMaximized,
    onMinimize,
    onToggleMaximize,
    onClose
  };
}
