import type { TFunction } from "i18next";
import { formatBytes, formatDate, formatDuration, formatMhz } from "../../lib/format";

/** Subset of `my_pc_info` used by the report. */
export interface PcInfo {
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

/** Subset of `disks_list`. */
export interface DiskInfo {
  name: string;
  mountPoint: string;
  fileSystem: string;
  kind: "ssd" | "hdd" | "unknown";
  removable: boolean;
  totalBytes: number;
  availableBytes: number;
}

/** Subset of `network_adapters`. */
export interface NetworkAdapter {
  name: string;
  description: string | null;
  kind: string;
  mac: string | null;
  ipv4: { address: string; netmask: string; prefix: number }[];
  ipv6: string[];
  gateway: string | null;
  dnsServers: string[];
  dhcp: boolean | null;
  speedBps: number | null;
  isUp: boolean;
  isDefault: boolean;
}

export interface SnapshotData {
  pc: PcInfo;
  disks: DiskInfo[];
  adapters: NetworkAdapter[];
}

const WIDTH = 60;
const LABEL_WIDTH = 22;
const RULE = "-".repeat(WIDTH);
const DOUBLE_RULE = "=".repeat(WIDTH);

/** "Label ........: value" with aligned values. */
const row = (label: string, value: string, indent = "") =>
  `${indent}${label} ${".".repeat(Math.max(2, LABEL_WIDTH - label.length))}: ${value}`;

const percent = (used: number, total: number) => (total ? `${Math.round((used / total) * 100)}%` : "0%");

/** Plain-text system report, localized with the "my-pc" namespace. */
export function buildReport(t: TFunction, { pc, disks, adapters }: SnapshotData, generated: Date): string {
  const none = t("report.none");
  const yesNo = (value: boolean | null) => (value === null ? none : value ? t("report.yes") : t("report.no"));
  const usage = (used: number, total: number) =>
    t("report.usage", { used: formatBytes(used), total: formatBytes(total), percent: percent(used, total) });
  const lines: string[] = [];

  lines.push(
    DOUBLE_RULE,
    `  ${t("report.title")}`,
    `  ${t("report.generated", { date: formatDate(generated.getTime()) })}`,
    `  ${pc.username || "user"}@${pc.hostname || "host"}`,
    DOUBLE_RULE,
    "",
  );

  lines.push(t("report.system"), RULE);
  lines.push(
    row(t("specs.os"), pc.osName || none),
    row(t("specs.kernel"), pc.kernel || none),
    row(t("specs.host"), pc.hostname || none),
    row(t("report.user"), pc.username || none),
    row(t("specs.arch"), pc.arch || none),
    row(t("specs.uptime"), formatDuration(pc.uptimeSecs)),
    row(t("specs.cpu"), pc.cpuBrand || none),
    row(
      t("specs.cores"),
      pc.cpuPhysicalCores
        ? t("coresValue", { physical: pc.cpuPhysicalCores, logical: pc.cpuLogicalCores })
        : t("threadsValue", { logical: pc.cpuLogicalCores }),
    ),
    row(t("report.frequency"), pc.cpuFrequencyMhz ? formatMhz(pc.cpuFrequencyMhz) : none),
    row(t("specs.gpu"), pc.gpus.length ? pc.gpus.join(" | ") : none),
    row(t("specs.memory"), usage(pc.memoryUsed, pc.memoryTotal)),
    row(t("specs.swap"), pc.swapTotal ? usage(pc.swapUsed, pc.swapTotal) : none),
    "",
  );

  lines.push(t("report.disks", { count: disks.length }), RULE);
  disks.forEach((disk, index) => {
    const used = disk.totalBytes - disk.availableBytes;
    const kind = t(`report.kind.${disk.kind}`) + (disk.removable ? ` · ${t("report.removable")}` : "");
    lines.push(
      `[${index + 1}] ${disk.name}${disk.name !== disk.mountPoint ? ` (${disk.mountPoint})` : ""}`,
      row(t("report.fileSystem"), disk.fileSystem || none, "    "),
      row(t("report.type"), kind, "    "),
      row(t("report.total"), formatBytes(disk.totalBytes), "    "),
      row(t("report.available"), formatBytes(disk.availableBytes), "    "),
      row(t("report.used"), `${formatBytes(used)} (${percent(used, disk.totalBytes)})`, "    "),
      "",
    );
  });

  lines.push(t("report.network", { count: adapters.length }), RULE);
  adapters.forEach((adapter, index) => {
    const status = [adapter.isUp ? t("report.up") : t("report.down"), adapter.isDefault ? t("report.default") : null]
      .filter(Boolean)
      .join(" · ");
    lines.push(
      `[${index + 1}] ${adapter.name} — ${status}`,
      row(t("report.description"), adapter.description ?? adapter.kind, "    "),
      ...(adapter.ipv4.length
        ? adapter.ipv4.map((ip) => row(t("report.ipv4"), `${ip.address} / ${ip.netmask} (/${ip.prefix})`, "    "))
        : [row(t("report.ipv4"), none, "    ")]),
      row(t("report.ipv6"), adapter.ipv6.length ? adapter.ipv6.join(", ") : none, "    "),
      row(t("report.mac"), adapter.mac ?? none, "    "),
      row(t("report.gateway"), adapter.gateway ?? none, "    "),
      row(t("report.dns"), adapter.dnsServers.length ? adapter.dnsServers.join(", ") : none, "    "),
      row(t("report.dhcp"), yesNo(adapter.dhcp), "    "),
      row(t("report.speed"), adapter.speedBps ? `${Math.round(adapter.speedBps / 1e6)} Mbps` : none, "    "),
      "",
    );
  });

  lines.push(DOUBLE_RULE, `  ${t("report.footer")}`, DOUBLE_RULE, "");
  return lines.join("\n");
}
