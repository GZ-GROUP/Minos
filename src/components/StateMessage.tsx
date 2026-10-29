import { useTranslation } from "react-i18next";
import { CircleNotch, Tray, WarningCircle } from "@phosphor-icons/react";
import { NOT_AVAILABLE } from "../hooks/useCommand";
import "./StateMessage.css";

interface StateMessageProps {
  kind: "loading" | "error" | "empty";
  /** Raw error from the backend, shown as detail under the friendly message. */
  error?: string | null;
  message?: string;
}

/** Shared loading / error / empty placeholder for data views. */
export function StateMessage({ kind, error, message }: StateMessageProps) {
  const { t } = useTranslation();

  if (kind === "loading") {
    return (
      <div className="state-message" role="status">
        <CircleNotch className="state-message__spinner" aria-hidden="true" />
        <p>{message ?? t("list.loading")}</p>
      </div>
    );
  }

  if (kind === "error") {
    const notAvailable = error === NOT_AVAILABLE;
    return (
      <div className="state-message state-message--error" role="alert">
        <WarningCircle aria-hidden="true" />
        <p>{message ?? (notAvailable ? t("list.notAvailable") : t("list.error"))}</p>
        {error && !notAvailable && <code className="state-message__detail">{error}</code>}
      </div>
    );
  }

  return (
    <div className="state-message">
      <Tray aria-hidden="true" />
      <p>{message ?? t("list.empty")}</p>
    </div>
  );
}
