import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { CheckCircle, Info, WarningCircle, type Icon } from "@phosphor-icons/react";
import { Button } from "../Button";
import { Dialog, type DialogTone } from "./Dialog";

export interface AlertOptions {
  title: string;
  message: ReactNode;
  tone?: DialogTone;
  icon?: Icon;
  okLabel?: string;
}

const TONE_ICONS: Record<DialogTone, Icon> = {
  default: Info,
  danger: WarningCircle,
  success: CheckCircle,
};

interface MessageBoxProps {
  options: AlertOptions;
  onResolve: () => void;
}

/** Informational message with a single OK button. */
export function MessageBox({ options, onResolve }: MessageBoxProps) {
  const { t } = useTranslation();
  const { title, message, tone = "default", icon, okLabel } = options;

  return (
    <Dialog
      open
      title={title}
      tone={tone}
      icon={icon ?? TONE_ICONS[tone]}
      onClose={onResolve}
      actions={
        <Button variant="primary" autoFocus onClick={onResolve}>
          {okLabel ?? t("dialog.ok")}
        </Button>
      }
    >
      {typeof message === "string" ? <p>{message}</p> : message}
    </Dialog>
  );
}
