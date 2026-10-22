import { Suspense, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft } from "@phosphor-icons/react";
import type { AppDefinition } from "../apps/types";
import "./AppView.css";

interface AppViewProps {
  app: AppDefinition;
  onBack: () => void;
}

/** Hosts a single app, lazy-loading its component. */
export function AppView({ app, onBack }: AppViewProps) {
  const { t } = useTranslation();
  const { t: tApp } = useTranslation(app.id);
  const backRef = useRef<HTMLButtonElement>(null);
  const { Component, icon: Icon } = app;

  useEffect(() => {
    backRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !document.querySelector("dialog[open]")) onBack();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onBack]);

  return (
    <section className="app-view" aria-labelledby="app-view-title">
      <header className="app-view__header">
        <button ref={backRef} type="button" className="clay-button" onClick={onBack}>
          <ArrowLeft size={18} weight="bold" aria-hidden="true" /> {t("appView.back")}
        </button>
        <span className="app-view__icon" aria-hidden="true">
          <Icon size={34} />
        </span>
        <div>
          <h2 id="app-view-title">{tApp("name")}</h2>
          <p className="app-view__description">{tApp("description")}</p>
        </div>
      </header>

      <div className="app-view__body">
        <Suspense fallback={null}>
          <Component />
        </Suspense>
      </div>
    </section>
  );
}
