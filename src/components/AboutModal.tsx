import { useTranslation } from "react-i18next";
import logo from "../assets/logo.png";
import { version } from "../../package.json";
import { Button } from "./Button";
import { Dialog } from "./dialogs/Dialog";
import "./AboutModal.css";

interface AboutModalProps {
  open: boolean;
  onClose: () => void;
}

export function AboutModal({ open, onClose }: AboutModalProps) {
  const { t } = useTranslation();

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
    >
      <p className="about-modal__version">{t("about.version", { version })}</p>
      <p>{t("about.body")}</p>
    </Dialog>
  );
}
