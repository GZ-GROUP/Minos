import type { ReactNode } from "react";
import { ArrowRight } from "@phosphor-icons/react";
import "./ConvertLayout.css";

interface ConvertLayoutProps {
  /** Left slot: what goes in (usually FileSelector in "select" mode). */
  input: ReactNode;
  /** Right slot: what comes out (usually FileSelector in "download" mode). */
  output: ReactNode;
  /** Options shown under the flow (e.g. a page pattern field). */
  controls?: ReactNode;
  /** Primary action, centered below. */
  action: ReactNode;
  /** Status line above the flow (e.g. which engine will run). */
  notice?: ReactNode;
}

/** Standard "input → output" layout for converter-style apps. */
export function ConvertLayout({ input, output, controls, action, notice }: ConvertLayoutProps) {
  return (
    <div className="convert">
      {notice && <div className="convert__notice">{notice}</div>}
      <div className="convert__flow">
        <div className="convert__slot">{input}</div>
        <span className="convert__arrow" aria-hidden="true">
          <ArrowRight weight="bold" />
        </span>
        <div className="convert__slot">{output}</div>
      </div>
      {controls && <div className="convert__controls">{controls}</div>}
      <div className="convert__action">{action}</div>
    </div>
  );
}
