import i18n from "../i18n";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

function number(value: number, maximumFractionDigits: number, minimumFractionDigits = 0): string {
  return new Intl.NumberFormat(i18n.language, { maximumFractionDigits, minimumFractionDigits }).format(value);
}

/** Binary units (1 KB = 1024 B), matching what Windows Explorer and most file managers show. */
export function formatBytes(bytes: number, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${number(value, exponent === 0 ? 0 : digits)} ${BYTE_UNITS[exponent]}`;
}

/** `value` is a 0–100 percentage. */
export function formatPercent(value: number, digits = 1): string {
  return new Intl.NumberFormat(i18n.language, {
    style: "percent",
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value / 100);
}

/** Compact duration such as "3d 4h 12m". */
export function formatDuration(totalSeconds: number): string {
  const days = Math.floor(totalSeconds / 86_400);
  const hours = Math.floor((totalSeconds % 86_400) / 3_600);
  const minutes = Math.floor((totalSeconds % 3_600) / 60);
  return [days && `${days}d`, (days || hours) && `${hours}h`, `${minutes}m`].filter(Boolean).join(" ");
}

/** Short localized date + time, e.g. "Oct 28, 2026, 9:41 PM". */
export function formatDate(epochMs: number): string {
  return new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }).format(epochMs);
}

export function formatBitrate(bitsPerSecond: number): string {
  if (bitsPerSecond >= 1e9) return `${number(bitsPerSecond / 1e9, 1)} Gbps`;
  if (bitsPerSecond >= 1e6) return `${number(bitsPerSecond / 1e6, 0)} Mbps`;
  return `${number(bitsPerSecond / 1e3, 0)} Kbps`;
}

export function formatMhz(mhz: number): string {
  return mhz >= 1000 ? `${number(mhz / 1000, 2, 2)} GHz` : `${number(mhz, 0)} MHz`;
}
