import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { apps } from "./apps/registry";
import { AppMenu } from "./components/AppMenu";
import { AppDrawer } from "./components/AppDrawer";
import { AppView } from "./components/AppView";
import { StateMessage } from "./components/StateMessage";
import { WindowControls } from "./components/WindowControls";
import { MarketplaceModal } from "./marketplace/MarketplaceModal";
import { useMarketplace } from "./marketplace/useMarketplace";
import "./App.css";

function App() {
  const { t } = useTranslation();
  const [activeAppId, setActiveAppId] = useState<string | null>(null);
  const [marketplaceOpen, setMarketplaceOpen] = useState(false);
  const market = useMarketplace();
  const closeApp = useCallback(() => setActiveAppId(null), []);
  const openMarketplace = useCallback(() => setMarketplaceOpen(true), []);
  const closeMarketplace = useCallback(() => setMarketplaceOpen(false), []);

  const installed = market.data?.installed;
  // In the order the user arranged them.
  const installedApps = useMemo(
    () => (installed ?? []).flatMap((id) => apps.filter((app) => app.id === id)),
    [installed],
  );
  const activeApp = installedApps.find((app) => app.id === activeAppId);
  const { reorder } = market;

  const reorderApps = useCallback(
    (from: number, to: number) => {
      const ids = installedApps.map((app) => app.id);
      const [moved] = ids.splice(from, 1);
      ids.splice(to, 0, moved);
      reorder(ids);
    },
    [installedApps, reorder],
  );

  // The open app was uninstalled: don't reopen it if it's installed again later.
  useEffect(() => {
    if (activeAppId && !activeApp) setActiveAppId(null);
  }, [activeAppId, activeApp]);

  return (
    <div className="shell">
      {/* Doubles as the window title bar: drag regions move the window, double-click maximizes. */}
      <header className="shell__header" data-tauri-drag-region>
        <AppMenu onOpenMarketplace={openMarketplace} />
        <h1 className="shell__title" data-tauri-drag-region>
          {t("appName")}
        </h1>
        <div className="shell__drag-spacer" data-tauri-drag-region />
        <WindowControls />
      </header>

      <main className="shell__main">
        {activeApp ? (
          <AppView app={activeApp} onBack={closeApp} />
        ) : market.loadError ? (
          <StateMessage kind="error" error={market.loadError} />
        ) : (
          // Waits for the saved state, so that installed apps don't flash the empty screen.
          market.data && (
            <AppDrawer
              apps={installedApps}
              onOpen={setActiveAppId}
              onOpenMarketplace={openMarketplace}
              onReorder={reorderApps}
            />
          )
        )}
      </main>

      <MarketplaceModal open={marketplaceOpen} onClose={closeMarketplace} apps={apps} market={market} />
    </div>
  );
}

export default App;
