import { useTranslation } from "react-i18next";
import type { AppDefinition } from "../apps/types";
import "./AppTile.css";

interface AppTileProps {
  app: AppDefinition;
  onOpen: (id: string) => void;
}

export function AppTile({ app, onOpen }: AppTileProps) {
  const { t } = useTranslation(app.id);
  const { icon: Icon } = app;

  return (
    <button
      type="button"
      className="app-tile clay"
      title={t("description")}
      onClick={() => onOpen(app.id)}
    >
      <span className="app-tile__content">
        <span className="app-tile__icon" aria-hidden="true">
          <Icon />
        </span>
        <span className="app-tile__name">{t("name")}</span>
      </span>
    </button>
  );
}
