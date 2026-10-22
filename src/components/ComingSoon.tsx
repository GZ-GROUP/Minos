import { useTranslation } from "react-i18next";
import "./ComingSoon.css";

/** Shared placeholder body for apps that have no functionality yet. */
export function ComingSoon() {
  const { t } = useTranslation();
  return (
    <div className="coming-soon clay">
      <span className="coming-soon__badge">{t("appView.comingSoon")}</span>
      <p>{t("appView.comingSoonBody")}</p>
    </div>
  );
}
