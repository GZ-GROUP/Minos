import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import "./Tooltip.css";

const SHOW_DELAY_MS = 350;
const VIEWPORT_MARGIN = 8;

interface TooltipProps {
  label: string;
  children: ReactNode;
  className?: string;
  /** Only show when the anchor text is cut off (element marked `data-truncate`, or the anchor). */
  whenTruncated?: boolean;
}

/**
 * Shows `label` in a floating bubble on hover or keyboard focus. Rendered in a
 * portal with fixed positioning so containers with `overflow: hidden` can't clip it.
 * The bubble is decorative (aria-hidden): the anchor must already expose the same text.
 */
export function Tooltip({ label, children, className, whenTruncated = false }: TooltipProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const bubbleRef = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  const show = () => {
    if (whenTruncated) {
      const anchor = anchorRef.current;
      const text = anchor?.querySelector<HTMLElement>("[data-truncate]") ?? anchor;
      if (!text || text.scrollWidth <= text.clientWidth) return;
    }
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setOpen(true), SHOW_DELAY_MS);
  };

  const hide = () => {
    window.clearTimeout(timer.current);
    setOpen(false);
    setPosition(null);
  };

  // Measure after the bubble renders so it can be centered and kept inside the window.
  useEffect(() => {
    if (!open) return;
    const anchor = anchorRef.current?.getBoundingClientRect();
    const bubble = bubbleRef.current?.getBoundingClientRect();
    if (!anchor || !bubble) return;
    const centered = anchor.left + anchor.width / 2 - bubble.width / 2;
    const left = Math.min(Math.max(centered, VIEWPORT_MARGIN), window.innerWidth - bubble.width - VIEWPORT_MARGIN);
    setPosition({ top: anchor.bottom + VIEWPORT_MARGIN, left });

    const onKeyDown = (event: KeyboardEvent) => event.key === "Escape" && hide();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", hide, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", hide, true);
    };
  }, [open]);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <span
      ref={anchorRef}
      className={className}
      onPointerEnter={show}
      onPointerLeave={hide}
      onPointerDown={hide}
      // Keyboard focus only; a mouse click also focuses but should not reopen the tooltip.
      onFocus={(event) => {
        if (event.target.matches(":focus-visible")) show();
      }}
      onBlur={hide}
    >
      {children}
      {open &&
        createPortal(
          <div
            ref={bubbleRef}
            className="tooltip"
            aria-hidden="true"
            data-visible={position !== null}
            style={position ?? undefined}
          >
            {label}
          </div>,
          document.body,
        )}
    </span>
  );
}
