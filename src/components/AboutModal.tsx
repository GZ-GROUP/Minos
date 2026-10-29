import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GithubLogo } from "@phosphor-icons/react";
import logo from "../assets/logo.png";
import { version } from "../../package.json";
import { Button } from "./Button";
import { Dialog } from "./dialogs/Dialog";
import "./AboutModal.css";

const REPOSITORY_URL = "https://github.com/GZ-GROUP/Minos";

interface AboutModalProps {
  open: boolean;
  onClose: () => void;
}

export function AboutModal({ open, onClose }: AboutModalProps) {
  const { t } = useTranslation();

  // Open in the system browser instead of navigating the app's own webview.
  const openRepository = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    if (isTauri()) openUrl(REPOSITORY_URL).catch(() => {});
    else window.open(REPOSITORY_URL, "_blank", "noopener");
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("about.title")}
      align="center"
      media={<img className="about-modal__logo" src={logo} alt="" />}
      actions={
        <Button variant="primary" autoFocus onClick={onClose}>
          {t("about.close")}
        </Button>
      }
      footer={
        <>
          <a className="about-modal__link" href={REPOSITORY_URL} onClick={openRepository}>
            <GithubLogo weight="fill" aria-hidden="true" />
            {t("about.github")}
          </a>
          <span aria-hidden="true">·</span>
          <span className="about-modal__version">{t("about.version", { version })}</span>
        </>
      }
    >
      <p>{t("about.description")}</p>
      <p>{t("about.developer")}</p>
    </Dialog>
  );
}
