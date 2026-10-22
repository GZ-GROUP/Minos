import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";
import { version } from "../../package.json";
import "./AboutModal.css";

interface AboutModalProps {
  open: boolean;
  onClose: () => void;
}

/** Uses the native <dialog> for focus trapping, Esc handling and top-layer rendering. */
export function AboutModal({ open, onClose }: AboutModalProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={dialogRef}
      className="about-modal clay"
      aria-labelledby="about-title"
      onClose={onClose}
      onClick={(event) => {
        // Clicking the backdrop targets the dialog element itself.
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="about-modal__content">
        <img className="about-modal__logo" src={logo} alt="" />
        <h2 id="about-title">{t("about.title")}</h2>
        <p className="about-modal__version">{t("about.version", { version })}</p>
        <p className="about-modal__body">{t("about.body")}</p>
        <button type="button" className="clay-button clay-button--primary" onClick={onClose} autoFocus>
          {t("about.close")}
        </button>
      </div>
    </dialog>
  );
}
