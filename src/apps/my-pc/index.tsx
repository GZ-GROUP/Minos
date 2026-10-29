import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { documentDir, join } from "@tauri-apps/api/path";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AppleLogo, Camera, CheckCircle, Desktop, LinuxLogo, WindowsLogo, type Icon } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { RefreshButton } from "../../components/RefreshButton";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { describeError } from "../../lib/errors";
import { buildReport, type DiskInfo, type NetworkAdapter } from "./report";
import { StateMessage } from "../../components/StateMessage";
import { UsageBar } from "../../components/UsageBar";
import { useCommand } from "../../hooks/useCommand";
import { formatBytes, formatDuration, formatMhz } from "../../lib/format";
import "./MyPc.css";

interface PcInfo {
  osFamily: string;
  osName: string;
  kernel: string;
  hostname: string;
  username: string;
  arch: string;
  uptimeSecs: number;
  cpuBrand: string;
  cpuPhysicalCores: number | null;
  cpuLogicalCores: number;
  cpuFrequencyMhz: number;
  memoryTotal: number;
  memoryUsed: number;
  swapTotal: number;
  swapUsed: number;
  gpus: string[];
}

const OS_LOGOS: Record<string, Icon> = {
  windows: WindowsLogo,
  linux: LinuxLogo,
  macos: AppleLogo,
};

/** Neofetch's trailing color blocks, using the brand palette. */
const SWATCHES = [
  "--color-primary",
  "--color-primary-strong",
  "--color-secondary",
  "--color-light",
  "--color-success",
  "--color-warning",
  "--color-danger",
  "--color-text-muted",
];

const pad = (value: number) => String(value).padStart(2, "0");

/** e.g. minos-snapshot-2026-10-28-2141.txt */
function snapshotName(date: Date): string {
  const day = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  return `minos-snapshot-${day}-${pad(date.getHours())}${pad(date.getMinutes())}.txt`;
}

export default function MyPc() {
  const { t } = useTranslation("my-pc");
  const { t: tCommon } = useTranslation();
  const { confirm, alert } = useDialogs();
  const { data, error, loading, refreshing, refresh } = useCommand<PcInfo>("my_pc_info");
  const [saving, setSaving] = useState(false);

  /** Saves a TXT report (this PC + disks + network) where the user chooses. */
  const snapshot = async () => {
    const now = new Date();
    let defaultPath = snapshotName(now);
    try {
      defaultPath = await join(await documentDir(), defaultPath);
    } catch {
      // No Documents folder: the dialog opens in its default location.
    }
    const chosen = await save({ defaultPath, filters: [{ name: t("report.fileType"), extensions: ["txt"] }] });
    if (!chosen) return;
    const destination = /.txt$/i.test(chosen) ? chosen : `${chosen}.txt`;

    setSaving(true);
    try {
      const [pc, disks, adapters] = await Promise.all([
        invoke<PcInfo>("my_pc_info"),
        invoke<DiskInfo[]>("disks_list"),
        invoke<NetworkAdapter[]>("network_adapters"),
      ]);
      await invoke("report_save", { path: destination, content: buildReport(t, { pc, disks, adapters }, now) });
      setSaving(false);
      const choice = await confirm({
        icon: CheckCircle,
        title: t("snapshotSaved"),
        message: destination,
        options: [
          { id: "show", label: t("showInFolder") },
          { id: "ok", label: tCommon("dialog.ok"), variant: "primary" },
        ],
        defaultOption: "ok",
      });
      if (choice === "show") revealItemInDir(destination).catch(() => {});
    } catch (cause) {
      setSaving(false);
      await alert({ tone: "danger", title: tCommon("errors.title"), message: describeError(cause) });
    }
  };

  if (loading && !data) return <StateMessage kind="loading" />;
  if (error && !data) return <StateMessage kind="error" error={error} />;
  if (!data) return null;

  const OsLogo = OS_LOGOS[data.osFamily] ?? Desktop;
  const memoryPercent = data.memoryTotal ? (data.memoryUsed / data.memoryTotal) * 100 : 0;
  const swapPercent = data.swapTotal ? (data.swapUsed / data.swapTotal) * 100 : 0;
  const unknown = t("unknown");

  const specs: { key: string; value: ReactNode }[] = [
    { key: "os", value: data.osName || unknown },
    { key: "kernel", value: data.kernel || unknown },
    { key: "host", value: data.hostname || unknown },
    { key: "uptime", value: formatDuration(data.uptimeSecs) },
    {
      key: "cpu",
      value: data.cpuBrand
        ? // Intel brand strings already include the base clock ("… @ 1.60GHz").
          `${data.cpuBrand}${data.cpuFrequencyMhz && !/hz/i.test(data.cpuBrand) ? ` @ ${formatMhz(data.cpuFrequencyMhz)}` : ""}`
        : unknown,
    },
    {
      key: "cores",
      value: data.cpuPhysicalCores
        ? t("coresValue", { physical: data.cpuPhysicalCores, logical: data.cpuLogicalCores })
        : t("threadsValue", { logical: data.cpuLogicalCores }),
    },
    {
      key: "gpu",
      value: data.gpus.length ? (
        <span className="my-pc__lines">
          {data.gpus.map((gpu) => (
            <span key={gpu}>{gpu}</span>
          ))}
        </span>
      ) : (
        unknown
      ),
    },
    {
      key: "memory",
      value: (
        <span className="my-pc__usage">
          {t("usedOf", { used: formatBytes(data.memoryUsed), total: formatBytes(data.memoryTotal) })}
          <UsageBar value={memoryPercent} label={t("specs.memory")} />
        </span>
      ),
    },
    {
      key: "swap",
      value: data.swapTotal ? (
        <span className="my-pc__usage">
          {t("usedOf", { used: formatBytes(data.swapUsed), total: formatBytes(data.swapTotal) })}
          <UsageBar value={swapPercent} label={t("specs.swap")} />
        </span>
      ) : (
        unknown
      ),
    },
    { key: "arch", value: data.arch || unknown },
  ];

  return (
    <div className="my-pc">
      <div className="my-pc__toolbar">
        <RefreshButton onClick={refresh} refreshing={refreshing} />
      </div>

      <article className="my-pc__card clay" aria-labelledby="my-pc-identity">
        <div className="my-pc__brand">
          <span className="my-pc__logo" aria-hidden="true">
            <OsLogo />
          </span>
          <h3 id="my-pc-identity" className="my-pc__identity">
            <span>{data.username || "user"}</span>
            <span className="my-pc__at">@</span>
            <span>{data.hostname || "host"}</span>
          </h3>
        </div>

        <dl className="my-pc__specs">
          {specs.map(({ key, value }) => (
            <div key={key} className="my-pc__spec">
              <dt>{t(`specs.${key}`)}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>

        <div className="my-pc__swatches" aria-hidden="true">
          {SWATCHES.map((token) => (
            <span key={token} style={{ background: `var(${token})` }} />
          ))}
        </div>
      </article>

      <div className="my-pc__footer">
        <Button variant="primary" icon={Camera} loading={saving} onClick={snapshot}>
          {t("snapshot")}
        </Button>
        <p className="my-pc__hint">{t("snapshotHint")}</p>
      </div>
    </div>
  );
}
