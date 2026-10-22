import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { apps } from "./apps/registry";
import { AppMenu } from "./components/AppMenu";
import { AppDrawer } from "./components/AppDrawer";
import { AppView } from "./components/AppView";
import { WindowControls } from "./components/WindowControls";
import "./App.css";

function App() {
  const { t } = useTranslation();
  const [activeAppId, setActiveAppId] = useState<string | null>(null);
  const activeApp = apps.find((app) => app.id === activeAppId);
  const closeApp = useCallback(() => setActiveAppId(null), []);

  return (
    <div className="shell">
      {/* Doubles as the window title bar: drag regions move the window, double-click maximizes. */}
      <header className="shell__header" data-tauri-drag-region>
        <AppMenu />
        <h1 className="shell__title" data-tauri-drag-region>
          {t("appName")}
        </h1>
        <div className="shell__drag-spacer" data-tauri-drag-region />
        <WindowControls />
      </header>

      <main className="shell__main">
        {activeApp ? (
          <AppView app={activeApp} onBack={closeApp} />
        ) : (
          <AppDrawer apps={apps} onOpen={setActiveAppId} />
        )}
      </main>
    </div>
  );
}

export default App;
