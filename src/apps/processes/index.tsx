import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { CellText, DataList, type Column } from "../../components/DataList";
import { StatusPill } from "../../components/StatusPill";
import { UsageBar } from "../../components/UsageBar";
import { useCommand } from "../../hooks/useCommand";
import { formatBytes, formatPercent } from "../../lib/format";
import "./Processes.css";

interface ProcessInfo {
  pid: number;
  name: string;
  cpuPercent: number;
  memoryBytes: number;
}

const REFRESH_MS = 2000;

export default function Processes() {
  const { t } = useTranslation("processes");
  const { t: tCommon } = useTranslation();
  const { data, error, loading } = useCommand<ProcessInfo[]>("processes_list", { intervalMs: REFRESH_MS });

  const columns = useMemo<Column<ProcessInfo>[]>(
    () => [
      {
        key: "name",
        header: t("columns.name"),
        render: (process) => <CellText primary={process.name} />,
        sortValue: (process) => process.name,
      },
      {
        key: "pid",
        header: t("columns.pid"),
        width: "90px",
        align: "end",
        render: (process) => process.pid,
        sortValue: (process) => process.pid,
      },
      {
        key: "cpu",
        header: t("columns.cpu"),
        width: "180px",
        align: "end",
        render: (process) => (
          <span className="processes__cpu">
            <UsageBar value={process.cpuPercent} size="sm" />
            {formatPercent(process.cpuPercent)}
          </span>
        ),
        sortValue: (process) => process.cpuPercent,
      },
      {
        key: "memory",
        header: t("columns.memory"),
        width: "120px",
        align: "end",
        render: (process) => formatBytes(process.memoryBytes),
        sortValue: (process) => process.memoryBytes,
      },
    ],
    [t],
  );

  return (
    <DataList
      label={t("name")}
      columns={columns}
      rows={data}
      rowKey={(process) => process.pid}
      loading={loading}
      error={error}
      searchText={(process) => `${process.name} ${process.pid}`}
      defaultSort={{ key: "cpu", direction: "desc" }}
      actions={
        !error && (
          <StatusPill tone="success" size="md" pulse>
            {tCommon("list.live")}
          </StatusPill>
        )
      }
    />
  );
}
