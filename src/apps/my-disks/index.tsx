import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { CellText, DataList, type Column } from "../../components/DataList";
import { RefreshButton } from "../../components/RefreshButton";
import { UsageBar } from "../../components/UsageBar";
import { useCommand } from "../../hooks/useCommand";
import { formatBytes, formatPercent } from "../../lib/format";
import "./MyDisks.css";

interface DiskInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  kind: "ssd" | "hdd" | "unknown";
  removable: boolean;
  totalBytes: number;
  availableBytes: number;
}

const usedPercent = (disk: DiskInfo) =>
  disk.totalBytes > 0 ? ((disk.totalBytes - disk.availableBytes) / disk.totalBytes) * 100 : 0;

export default function MyDisks() {
  const { t } = useTranslation("my-disks");
  const { data, error, loading, refreshing, refresh } = useCommand<DiskInfo[]>("disks_list");

  const columns = useMemo<Column<DiskInfo>[]>(
    () => [
      {
        key: "disk",
        header: t("columns.disk"),
        render: (disk) => (
          <CellText primary={disk.name} secondary={disk.name !== disk.mountPoint ? disk.mountPoint : undefined} />
        ),
        sortValue: (disk) => disk.name,
      },
      {
        key: "fileSystem",
        header: t("columns.fileSystem"),
        width: "120px",
        render: (disk) => disk.fileSystem,
        sortValue: (disk) => disk.fileSystem,
      },
      {
        key: "kind",
        header: t("columns.kind"),
        width: "110px",
        render: (disk) => (
          <CellText primary={t(`kind.${disk.kind}`)} secondary={disk.removable ? t("kind.removable") : undefined} />
        ),
        sortValue: (disk) => disk.kind,
      },
      {
        key: "total",
        header: t("columns.total"),
        width: "110px",
        align: "end",
        render: (disk) => formatBytes(disk.totalBytes),
        sortValue: (disk) => disk.totalBytes,
      },
      {
        key: "available",
        header: t("columns.available"),
        width: "110px",
        align: "end",
        render: (disk) => formatBytes(disk.availableBytes),
        sortValue: (disk) => disk.availableBytes,
      },
      {
        key: "used",
        header: t("columns.used"),
        width: "180px",
        align: "end",
        render: (disk) => (
          <span className="my-disks__used">
            <UsageBar value={usedPercent(disk)} size="sm" />
            {formatPercent(usedPercent(disk), 0)}
          </span>
        ),
        sortValue: usedPercent,
      },
    ],
    [t],
  );

  return (
    <DataList
      label={t("name")}
      columns={columns}
      rows={data}
      rowKey={(disk) => disk.mountPoint}
      loading={loading}
      error={error}
      actions={<RefreshButton onClick={refresh} refreshing={refreshing} />}
    />
  );
}
