import type { ReactNode } from "react";
import "./StatusPill.css";

interface StatusPillProps {
  tone?: "success" | "warning" | "danger" | "neutral";
  /** Animates the dot, e.g. for live-updating data. */
  pulse?: boolean;
  /** "md" matches the 40px height of toolbar controls. */
  size?: "sm" | "md";
  children: ReactNode;
}

export function StatusPill({ tone = "neutral", pulse = false, size = "sm", children }: StatusPillProps) {
  return (
    <span className={`status-pill status-pill--${size}`} data-tone={tone} data-pulse={pulse}>
      <span className="status-pill__dot" aria-hidden="true" />
      {children}
    </span>
  );
}
