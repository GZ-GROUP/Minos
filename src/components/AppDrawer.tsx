import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Storefront } from "@phosphor-icons/react";
import type { AppDefinition } from "../apps/types";
import { useGridReorder } from "../hooks/useGridReorder";
import { AppTile } from "./AppTile";
import { Button } from "./Button";
import "./AppDrawer.css";

const COLUMNS = 4;
const ROWS = 3;
const PAGE_SIZE = COLUMNS * ROWS;
const WHEEL_COOLDOWN_MS = 450;

interface AppDrawerProps {
  apps: AppDefinition[];
  onOpen: (id: string) => void;
  onOpenMarketplace: () => void;
  /** Moves the app at position `from` to position `to` in `apps`. */
  onReorder: (from: number, to: number) => void;
}

/** Paged 4×3 grid of app tiles, reordered by pressing and dragging them. */
export function AppDrawer({ apps, onOpen, onOpenMarketplace, onReorder }: AppDrawerProps) {
  const { t } = useTranslation();
  const [requestedPage, setPage] = useState(0);
  const lastWheel = useRef(0);

  const pages = useMemo(() => {
    const chunks: AppDefinition[][] = [];
    for (let i = 0; i < apps.length; i += PAGE_SIZE) chunks.push(apps.slice(i, i + PAGE_SIZE));
    return chunks;
  }, [apps]);

  const pageCount = pages.length;
  // Uninstalling apps can remove the page being shown.
  const page = Math.min(requestedPage, Math.max(pageCount - 1, 0));
  const goTo = (target: number) => setPage(Math.max(0, Math.min(pageCount - 1, target)));
  const { dragging, itemProps } = useGridReorder((from, to) =>
    onReorder(page * PAGE_SIZE + from, page * PAGE_SIZE + to),
  );

  useEffect(() => {
    if (pageCount <= 1 || dragging) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.querySelector("dialog[open]")) return;
      if (event.key === "ArrowRight" || event.key === "PageDown") setPage((p) => Math.min(pageCount - 1, p + 1));
      if (event.key === "ArrowLeft" || event.key === "PageUp") setPage((p) => Math.max(0, p - 1));
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [pageCount, dragging]);

  const onWheel = (event: React.WheelEvent) => {
    if (pageCount <= 1 || dragging) return;
    const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    const now = Date.now();
    if (Math.abs(delta) < 20 || now - lastWheel.current < WHEEL_COOLDOWN_MS) return;
    lastWheel.current = now;
    goTo(page + Math.sign(delta));
  };

  if (pageCount === 0) {
    return (
      <div className="app-drawer__empty">
        <span className="app-drawer__empty-icon" aria-hidden="true">
          <Storefront />
        </span>
        <h2>{t("drawer.emptyTitle")}</h2>
        <p>{t("drawer.emptyBody")}</p>
        <Button variant="primary" icon={Storefront} onClick={onOpenMarketplace}>
          {t("drawer.openMarketplace")}
        </Button>
      </div>
    );
  }

  return (
    <section className="app-drawer" aria-label={t("drawer.label")} onWheel={onWheel}>
      <div className="app-drawer__viewport">
        <div className="app-drawer__track" style={{ transform: `translateX(-${page * 100}%)` }}>
          {pages.map((pageApps, index) => (
            <div
              key={index}
              className="app-drawer__page"
              inert={index !== page}
              aria-label={pageCount > 1 ? t("drawer.page", { page: index + 1, total: pageCount }) : undefined}
              role={pageCount > 1 ? "group" : undefined}
            >
              <ul className="app-drawer__grid">
                {pageApps.map((app, position) => (
                  <li key={app.id} {...(index === page ? itemProps(position) : {})}>
                    <AppTile app={app} onOpen={onOpen} />
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>

      {pageCount > 1 && (
        <nav className="app-drawer__dots" aria-label={t("drawer.label")}>
          {pages.map((_, index) => (
            <button
              key={index}
              type="button"
              className="app-drawer__dot"
              aria-label={t("drawer.page", { page: index + 1, total: pageCount })}
              aria-current={index === page ? "page" : undefined}
              onClick={() => goTo(index)}
            />
          ))}
        </nav>
      )}
    </section>
  );
}
