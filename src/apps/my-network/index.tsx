import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { CellText, DataList, type Column } from "../../components/DataList";
import { RefreshButton } from "../../components/RefreshButton";
import { useCommand } from "../../hooks/useCommand";
import { formatBitrate } from "../../lib/format";
import "./MyNetwork.css";

interface Ipv4Info {
  address: string;
  netmask: string;
  prefix: number;
}

interface NetworkAdapter {
  index: number;
  name: string;
  description: string | null;
  kind: string;
  mac: string | null;
  ipv4: Ipv4Info[];
  ipv6: string[];
  gateway: string | null;
  dnsServers: string[];
  dhcp: boolean | null;
  mtu: number | null;
  speedBps: number | null;
  isUp: boolean;
  isDefault: boolean;
}

/** Prefer a routable IPv6 address over link-local (fe80::/10) for the secondary line. */
const primaryIpv6 = (adapter: NetworkAdapter) =>
  adapter.ipv6.find((address) => !address.toLowerCase().startsWith("fe80")) ?? adapter.ipv6[0];

export default function MyNetwork() {
  const { t } = useTranslation("my-network");
  const { data, error, loading, refreshing, refresh } = useCommand<NetworkAdapter[]>("network_adapters");

  const columns = useMemo<Column<NetworkAdapter>[]>(
    () => [
      {
        key: "adapter",
        header: t("columns.adapter"),
        render: (adapter) => {
          const status = adapter.isUp ? t("status.up") : t("status.down");
          const details = [adapter.description ?? adapter.kind, adapter.isUp && adapter.speedBps ? formatBitrate(adapter.speedBps) : null];
          return (
            <span className="my-network__adapter">
              <span className="my-network__dot" data-up={adapter.isUp} role="img" aria-label={status} title={status} />
              <CellText
                primary={
                  <>
                    {adapter.name}
                    {adapter.isDefault && <span className="my-network__badge">{t("default")}</span>}
                  </>
                }
                secondary={details.filter(Boolean).join(" · ")}
              />
            </span>
          );
        },
        // Default route first, then connected adapters, then by name.
        sortValue: (adapter) => `${adapter.isDefault ? 0 : adapter.isUp ? 1 : 2} ${adapter.name}`,
      },
      {
        key: "ipv4",
        header: t("columns.ipv4"),
        width: "165px",
        render: (adapter) => (
          <CellText mono primary={adapter.ipv4[0]?.address ?? t("none")} secondary={primaryIpv6(adapter)} />
        ),
        sortValue: (adapter) => adapter.ipv4[0]?.address ?? "",
      },
      {
        key: "mask",
        header: t("columns.mask"),
        width: "135px",
        render: (adapter) => {
          const ipv4 = adapter.ipv4[0];
          return ipv4 ? (
            <CellText
              mono
              primary={ipv4.netmask}
              secondary={`/${ipv4.prefix} · ${adapter.dhcp ? t("dhcp") : t("static")}`}
            />
          ) : (
            t("none")
          );
        },
      },
      {
        key: "mac",
        header: t("columns.mac"),
        width: "150px",
        render: (adapter) => <CellText mono primary={adapter.mac ?? t("none")} />,
        sortValue: (adapter) => adapter.mac ?? "",
      },
      {
        key: "gateway",
        header: t("columns.gateway"),
        width: "125px",
        render: (adapter) => <CellText mono primary={adapter.gateway ?? t("none")} />,
        sortValue: (adapter) => adapter.gateway ?? "",
      },
    ],
    [t],
  );

  return (
    <DataList
      label={t("name")}
      columns={columns}
      rows={data}
      rowKey={(adapter) => adapter.index}
      loading={loading}
      error={error}
      minWidth={820}
      searchText={(adapter) =>
        [adapter.name, adapter.description, adapter.mac, adapter.gateway, ...adapter.ipv4.map((ip) => ip.address), ...adapter.ipv6]
          .filter(Boolean)
          .join(" ")
      }
      actions={<RefreshButton onClick={refresh} refreshing={refreshing} />}
    />
  );
}
