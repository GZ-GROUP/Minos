import "./UsageBar.css";

const HIGH_USAGE = 90;

interface UsageBarProps {
  /** 0–100 */
  value: number;
  /** Accessible name. Omit when the percentage is already shown as text next to the bar. */
  label?: string;
  size?: "sm" | "md";
}

export function UsageBar({ value, label, size = "md" }: UsageBarProps) {
  const clamped = Math.min(100, Math.max(0, value));
  const a11y = label
    ? { role: "meter", "aria-label": label, "aria-valuemin": 0, "aria-valuemax": 100, "aria-valuenow": Math.round(clamped) }
    : { "aria-hidden": true };

  return (
    <div className={`usage-bar usage-bar--${size}`} data-high={clamped >= HIGH_USAGE} {...a11y}>
      <span className="usage-bar__fill" style={{ width: `${clamped}%` }} />
    </div>
  );
}
