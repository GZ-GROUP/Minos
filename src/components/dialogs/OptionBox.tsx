import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Question, Warning, type Icon } from "@phosphor-icons/react";
import { Button, type ButtonVariant } from "../Button";
import { Dialog } from "./Dialog";

export interface ConfirmOption {
  id: string;
  label: string;
  variant?: ButtonVariant;
}

export interface ConfirmOptions {
  title: string;
  message: ReactNode;
  tone?: "default" | "danger";
  icon?: Icon;
  /** Buttons, left to right. Defaults to Cancel + Confirm. */
  options?: ConfirmOption[];
  confirmLabel?: string;
  cancelLabel?: string;
  /** Option focused on open. Danger boxes default to "cancel" so Enter is safe. */
  defaultOption?: string;
}

interface OptionBoxProps {
  options: ConfirmOptions;
  /** Chosen option id, or null when dismissed (Esc / backdrop). */
  onResolve: (choice: string | null) => void;
}

/** Message with a set of choices, e.g. a destructive-action confirmation. */
export function OptionBox({ options, onResolve }: OptionBoxProps) {
  const { t } = useTranslation();
  const { title, message, tone = "default", icon, confirmLabel, cancelLabel } = options;
  const choices: ConfirmOption[] = options.options ?? [
    { id: "cancel", label: cancelLabel ?? t("dialog.cancel"), variant: "secondary" },
    { id: "confirm", label: confirmLabel ?? t("dialog.confirm"), variant: tone === "danger" ? "danger" : "primary" },
  ];
  const focused = options.defaultOption ?? (tone === "danger" ? "cancel" : choices[choices.length - 1]?.id);

  return (
    <Dialog
      open
      title={title}
      tone={tone}
      icon={icon ?? (tone === "danger" ? Warning : Question)}
      onClose={() => onResolve(null)}
      actions={choices.map((choice) => (
        <Button
          key={choice.id}
          variant={choice.variant ?? "secondary"}
          autoFocus={choice.id === focused}
          onClick={() => onResolve(choice.id)}
        >
          {choice.label}
        </Button>
      ))}
    >
      {typeof message === "string" ? <p>{message}</p> : message}
    </Dialog>
  );
}
