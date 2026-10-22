import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppDefinition } from "../apps/types";
import { AppTile } from "./AppTile";
import "./AppDrawer.css";

const COLUMNS = 4;
const ROWS = 3;
const PAGE_SIZE = COLUMNS * ROWS;
const WHEEL_COOLDOWN_MS = 450;

interface AppDrawerProps {
  apps: AppDefinition[];
  onOpen: (id: string) => void;
}

/** Paged 4×3 grid of app tiles. */
export function AppDrawer({ apps, onOpen }: AppDrawerProps) {
  const { t } = useTranslation();
  const [page, setPage] = useState(0);
  const lastWheel = useRef(0);

  const pages = useMemo(() => {
    const chunks: AppDefinition[][] = [];
    for (let i = 0; i < apps.length; i += PAGE_SIZE) chunks.push(apps.slice(i, i + PAGE_SIZE));
    return chunks;
  }, [apps]);

  const pageCount = pages.length;
  const goTo = (target: number) => setPage(Math.max(0, Math.min(pageCount - 1, target)));

  useEffect(() => {
    if (pageCount <= 1) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.querySelector("dialog[open]")) return;
      if (event.key === "ArrowRight" || event.key === "PageDown") setPage((p) => Math.min(pageCount - 1, p + 1));
      if (event.key === "ArrowLeft" || event.key === "PageUp") setPage((p) => Math.max(0, p - 1));
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [pageCount]);

  const onWheel = (event: React.WheelEvent) => {
    if (pageCount <= 1) return;
    const delta = Math.abs(event.deltaX) > Math.abs(event.deltaY) ? event.deltaX : event.deltaY;
    const now = Date.now();
    if (Math.abs(delta) < 20 || now - lastWheel.current < WHEEL_COOLDOWN_MS) return;
    lastWheel.current = now;
    goTo(page + Math.sign(delta));
  };

  if (pageCount === 0) {
    return <p className="app-drawer__empty">{t("drawer.empty")}</p>;
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
                {pageApps.map((app) => (
                  <li key={app.id}>
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
