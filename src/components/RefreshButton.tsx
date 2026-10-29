import { useTranslation } from "react-i18next";
import { ArrowClockwise } from "@phosphor-icons/react";
import { Button } from "./Button";

interface RefreshButtonProps {
  onClick: () => void;
  refreshing?: boolean;
}

export function RefreshButton({ onClick, refreshing = false }: RefreshButtonProps) {
  const { t } = useTranslation();
  return (
    <Button icon={ArrowClockwise} loading={refreshing} onClick={onClick}>
      {t("list.refresh")}
    </Button>
  );
}
