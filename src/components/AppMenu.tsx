import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { CaretRight, Globe, Info, Moon, Sun } from "@phosphor-icons/react";
import logo from "../assets/logo.png";
import { useTheme } from "../theme/ThemeProvider";
import { Switch } from "./Switch";
import { LanguageSelect } from "./LanguageSelect";
import { AboutModal } from "./AboutModal";
import "./AppMenu.css";

/** Logo button that toggles a settings popover (theme, language, about). */
export function AppMenu() {
  const { t } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [open, setOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelId = useId();
  const darkLabelId = useId();
  const languageLabelId = useId();

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        // Keep window-level Escape handlers (e.g. AppView "back") from also firing.
        event.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus();
      }
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const openAbout = () => {
    setOpen(false);
    setAboutOpen(true);
  };

  const closeAbout = () => {
    setAboutOpen(false);
    triggerRef.current?.focus();
  };

  return (
    <div className="app-menu" ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className="app-menu__trigger"
        aria-label={t("menu.open")}
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen((value) => !value)}
      >
        <img src={logo} alt="" />
      </button>

      <div id={panelId} className="app-menu__panel clay" data-open={open} hidden={!open}>
        <div className="app-menu__row">
          <span id={darkLabelId} className="app-menu__label">
            {theme === "dark" ? <Moon aria-hidden="true" /> : <Sun aria-hidden="true" />}
            {t("menu.darkMode")}
          </span>
          <Switch
            checked={theme === "dark"}
            onChange={(checked) => setTheme(checked ? "dark" : "light")}
            labelledBy={darkLabelId}
          />
        </div>

        <div className="app-menu__row">
          <span id={languageLabelId} className="app-menu__label">
            <Globe aria-hidden="true" />
            {t("menu.language")}
          </span>
          <LanguageSelect labelledBy={languageLabelId} />
        </div>

        <button type="button" className="app-menu__row app-menu__row--action" onClick={openAbout}>
          <span className="app-menu__label">
            <Info aria-hidden="true" />
            {t("menu.about")}
          </span>
          <CaretRight className="app-menu__chevron" weight="bold" aria-hidden="true" />
        </button>
      </div>

      <AboutModal open={aboutOpen} onClose={closeAbout} />
    </div>
  );
}
