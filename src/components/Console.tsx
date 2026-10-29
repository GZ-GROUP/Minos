import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Broom, Check, Copy } from "@phosphor-icons/react";
import { Button } from "./Button";
import "./Console.css";

export type ConsoleLineKind = "out" | "err" | "info" | "success" | "input";

export interface ConsoleLine {
  id: number;
  text: string;
  kind: ConsoleLineKind;
}

interface ConsoleProps {
  title: string;
  lines: ConsoleLine[];
  /** Read-only consoles only display output; interactive ones add a prompt line. */
  readOnly?: boolean;
  /** Shows a blinking cursor while output is still arriving. */
  busy?: boolean;
  /** Right side of the header, e.g. a StatusPill. */
  status?: ReactNode;
  /** Interactive mode: called with each submitted command. */
  onSubmit?: (command: string) => void;
  /** Shows a "Clear" action when provided. */
  onClear?: () => void;
  prompt?: string;
  /** Shown while there is no output yet. */
  placeholder?: string;
}

/** Distance (px) from the bottom within which new output keeps the view pinned. */
const PIN_THRESHOLD = 24;

/** Terminal-style output view: black screen inside a clay frame. */
export function Console({
  title,
  lines,
  readOnly = true,
  busy = false,
  status,
  onSubmit,
  onClear,
  prompt = "$",
  placeholder,
}: ConsoleProps) {
  const { t } = useTranslation();
  const screenRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const pinned = useRef(true);
  const [command, setCommand] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [copied, setCopied] = useState(false);

  // Follow new output, unless the user scrolled up to read something.
  useLayoutEffect(() => {
    const screen = screenRef.current;
    if (screen && pinned.current) screen.scrollTop = screen.scrollHeight;
  }, [lines, busy]);

  const copy = async () => {
    await navigator.clipboard.writeText(lines.map((line) => line.text).join("\n"));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      const value = command.trim();
      if (!value) return;
      onSubmit?.(value);
      setHistory((previous) => [...previous, value]);
      setHistoryIndex(null);
      setCommand("");
      pinned.current = true;
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      if (!history.length) return;
      event.preventDefault();
      const last = history.length - 1;
      const next =
        event.key === "ArrowUp"
          ? Math.max(0, (historyIndex ?? history.length) - 1)
          : historyIndex === null || historyIndex >= last
            ? null
            : historyIndex + 1;
      setHistoryIndex(next);
      setCommand(next === null ? "" : history[next]);
    }
  };

  return (
    <section className="console clay" aria-label={title}>
      <header className="console__header">
        <span className="console__dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </span>
        <h3 className="console__title">{title}</h3>
        <div className="console__status">{status}</div>
        <div className="console__actions">
          <Button
            iconOnly
            size="sm"
            variant="ghost"
            icon={copied ? Check : Copy}
            label={t("console.copy")}
            disabled={!lines.length}
            onClick={copy}
          />
          {onClear && (
            <Button
              iconOnly
              size="sm"
              variant="ghost"
              icon={Broom}
              label={t("console.clear")}
              disabled={!lines.length || busy}
              onClick={onClear}
            />
          )}
        </div>
      </header>

      {/* The outer screen clips (rounded corners); the inner element scrolls. */}
      <div className="console__screen">
        <div
          ref={screenRef}
          className="console__scroll"
          // Focusable so keyboard users can scroll a read-only log.
          tabIndex={readOnly ? 0 : -1}
          onScroll={(event) => {
            const screen = event.currentTarget;
            pinned.current = screen.scrollHeight - screen.scrollTop - screen.clientHeight < PIN_THRESHOLD;
          }}
          onClick={() => !readOnly && inputRef.current?.focus()}
        >
          <div className="console__log" role="log" aria-live="polite" aria-busy={busy || undefined}>
            {lines.length === 0 && placeholder && <p className="console__placeholder">{placeholder}</p>}
            {lines.map((line) => (
              <div key={line.id} className="console__line" data-kind={line.kind}>
                {line.kind === "input" && <span className="console__prompt">{prompt} </span>}
                {line.text || " "}
              </div>
            ))}
            {busy && <span className="console__cursor" aria-hidden="true" />}
          </div>

          {!readOnly && (
            <label className="console__input-line">
              <span className="console__prompt" aria-hidden="true">
                {prompt}
              </span>
              <input
                ref={inputRef}
                className="console__input"
                value={command}
                aria-label={t("console.input")}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => setCommand(event.target.value)}
                onKeyDown={onKeyDown}
              />
            </label>
          )}
        </div>
      </div>
    </section>
  );
}
