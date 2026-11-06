import { useTranslation } from "react-i18next";
import { DownloadSimple, Plus, Trash } from "@phosphor-icons/react";
import type { AppIcon, Platform } from "../apps/types";
import { Button } from "../components/Button";
import { StatusPill } from "../components/StatusPill";
import { useDialogs } from "../components/dialogs/DialogProvider";
import { ErrorNote } from "./ErrorNote";
import type { Marketplace } from "./useMarketplace";

export interface MarketplaceItem {
  id: string;
  kind: "integrated" | "external";
  name: string;
  description: string;
  icon: AppIcon;
  /** Systems it works on; undefined means all of them. */
  platforms?: Platform[];
  /** Who made it and where it comes from, as shown on the card. */
  meta: string[];
  repo: string | null;
  /** Kept on this device, so it can be removed. */
  local: boolean;
  downloaded: boolean;
}

const PLATFORM_NAMES: Record<Platform, string> = {
  windows: "Windows",
  macos: "macOS",
  linux: "Linux",
};

interface ModuleCardProps {
  item: MarketplaceItem;
  market: Marketplace;
}

export function ModuleCard({ item, market }: ModuleCardProps) {
  const { t } = useTranslation();
  const dialogs = useDialogs();
  const { icon: Icon, id } = item;

  const installed = market.data?.installed.includes(id) ?? false;
  const supported =
    !item.platforms || (market.data != null && item.platforms.includes(market.data.platform));
  const downloading = id in market.downloads;
  const percent = market.downloads[id] ?? null;
  const error = market.errors[id];

  const confirmRemove = async () => {
    const choice = await dialogs.confirm({
      title: t("marketplace.remove.title", { name: item.name }),
      message: t(item.downloaded ? "marketplace.remove.downloaded" : "marketplace.remove.added"),
      tone: "danger",
      confirmLabel: t("marketplace.remove.confirm"),
    });
    if (choice === "confirm") market.remove(id);
  };

  const onlyFor = item.platforms?.map((platform) => PLATFORM_NAMES[platform]).join(", ");

  return (
    <li className="module-card">
      <span className="module-card__icon" aria-hidden="true">
        <Icon />
      </span>

      <div className="module-card__text">
        <div className="module-card__title">
          <h3>{item.name}</h3>
          {installed && <StatusPill tone="success">{t("marketplace.status.installed")}</StatusPill>}
          {item.downloaded && <StatusPill>{t("marketplace.status.downloaded")}</StatusPill>}
        </div>
        <p>{item.description}</p>
        <p className="module-card__meta">{item.meta.join(" · ")}</p>
        {item.downloaded && <p className="module-card__meta">{t("marketplace.status.installSoon")}</p>}
        {!supported && <p className="module-card__unsupported">{t("marketplace.onlyFor", { platforms: onlyFor })}</p>}
        {downloading && (
          <div
            className="module-card__progress"
            role="progressbar"
            aria-label={t("marketplace.action.downloading", { name: item.name })}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent ?? undefined}
            data-indeterminate={percent === null}
          >
            <span style={percent === null ? undefined : { width: `${percent}%` }} />
          </div>
        )}
        {error != null && <ErrorNote error={error} />}
      </div>

      <div className="module-card__actions">
        {item.kind === "integrated" &&
          (installed ? (
            <Button size="sm" onClick={() => market.setInstalled(id, false)}>
              {t("marketplace.action.uninstall")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="primary"
              icon={Plus}
              disabled={!supported}
              onClick={() => market.setInstalled(id, true)}
            >
              {t("marketplace.action.install")}
            </Button>
          ))}

        {item.kind === "external" && !item.downloaded && (
          <Button
            size="sm"
            variant="primary"
            icon={DownloadSimple}
            loading={downloading}
            disabled={!supported || !item.repo}
            onClick={() => item.repo && market.download(item.repo, id)}
          >
            {downloading && percent !== null ? `${percent}%` : t("marketplace.action.download")}
          </Button>
        )}

        {item.local && (
          <Button
            size="sm"
            variant="ghost"
            icon={Trash}
            iconOnly
            label={t("marketplace.action.remove", { name: item.name })}
            disabled={downloading}
            onClick={confirmRemove}
          />
        )}
      </div>
    </li>
  );
}
