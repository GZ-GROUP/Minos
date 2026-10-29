import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { AppleLogo, Desktop, LinuxLogo, WindowsLogo, type Icon } from "@phosphor-icons/react";
import { RefreshButton } from "../../components/RefreshButton";
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

export default function MyPc() {
  const { t } = useTranslation("my-pc");
  const { data, error, loading, refreshing, refresh } = useCommand<PcInfo>("my_pc_info");

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
    </div>
  );
}
