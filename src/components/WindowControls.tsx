import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CopySimple, Minus, Square, X } from "@phosphor-icons/react";
import "./WindowControls.css";

/** Custom minimize / maximize / close buttons for the undecorated Tauri window. */
export function WindowControls() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    const sync = () => appWindow.isMaximized().then(setMaximized);
    sync();
    const unlisten = appWindow.onResized(sync);
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Outside Tauri (e.g. `npm run dev` in a browser) there is no native window to control.
  if (!isTauri()) return null;

  const appWindow = getCurrentWindow();
  const maximizeLabel = maximized ? t("window.restore") : t("window.maximize");

  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-controls__button"
        aria-label={t("window.minimize")}
        title={t("window.minimize")}
        onClick={() => appWindow.minimize()}
      >
        <Minus weight="bold" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-controls__button"
        aria-label={maximizeLabel}
        title={maximizeLabel}
        onClick={() => appWindow.toggleMaximize()}
      >
        {maximized ? (
          <CopySimple weight="bold" aria-hidden="true" />
        ) : (
          <Square weight="bold" aria-hidden="true" />
        )}
      </button>
      <button
        type="button"
        className="window-controls__button window-controls__button--close"
        aria-label={t("window.close")}
        title={t("window.close")}
        onClick={() => appWindow.close()}
      >
        <X weight="bold" aria-hidden="true" />
      </button>
    </div>
  );
}
