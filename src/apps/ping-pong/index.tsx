import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { PingPong as PingIcon, StopCircle } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { Console, type ConsoleLine, type ConsoleLineKind } from "../../components/Console";
import { NumericUpDown } from "../../components/NumericUpDown";
import { StateMessage } from "../../components/StateMessage";
import { StatusPill } from "../../components/StatusPill";
import { TextField } from "../../components/TextField";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { describeError } from "../../lib/errors";
import "./PingPong.css";

/** Mirrors `PingEvent` in src-tauri/src/apps/ping_pong.rs. */
type PingEvent =
  | { kind: "line"; text: string; stream: "out" | "err" }
  | { kind: "done"; code: number | null; stopped: boolean };

type Outcome = "running" | "ok" | "failed" | "stopped";

const MAX_COUNT = 100;
const HOST_KEY = "minos.ping.host";
/** Same rule as `validate_host` in the backend. */
const HOST_PATTERN = /^(?!-)[A-Za-z0-9.:-]{1,253}$/;

function savedHost(): string {
  try {
    return localStorage.getItem(HOST_KEY) ?? "";
  } catch {
    return "";
  }
}

export default function PingPong() {
  const { t } = useTranslation("ping-pong");
  const [host, setHost] = useState(savedHost);
  const [count, setCount] = useState(4);
  const [hostError, setHostError] = useState<string | null>(null);
  const [lines, setLines] = useState<ConsoleLine[]>([]);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const nextId = useRef(0);

  // Leaving the app stops a ping that is still running.
  useEffect(
    () => () => {
      if (isTauri()) invoke("ping_stop").catch(() => {});
    },
    [],
  );

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const running = outcome === "running";
  const write = (text: string, kind: ConsoleLineKind) =>
    setLines((previous) => [...previous, { id: nextId.current++, text, kind }]);

  const ping = async () => {
    const target = host.trim();
    if (!HOST_PATTERN.test(target)) {
      setHostError(t("invalidHost"));
      return;
    }
    try {
      localStorage.setItem(HOST_KEY, target);
    } catch {
      // Remembering the host is only a convenience.
    }
    if (lines.length) write("", "out");
    write(t("started", { host: target, count }), "info");
    setOutcome("running");

    const channel = new Channel<PingEvent>();
    channel.onmessage = (event) => {
      if (event.kind === "line") {
        write(event.text, event.stream === "err" ? "err" : "out");
        return;
      }
      // Only a "done" event finishes a run; ignore anything unexpected.
      if (event.kind !== "done") return;
      if (event.stopped) {
        write(t("stoppedLine"), "info");
        setOutcome("stopped");
      } else if (event.code === 0) {
        write(t("finished", { code: event.code }), "success");
        setOutcome("ok");
      } else {
        write(t("failedLine", { code: event.code ?? "?" }), "err");
        setOutcome("failed");
      }
    };

    try {
      await invoke("ping_run", { host: target, count, onEvent: channel });
    } catch (error) {
      write(describeError(error), "err");
      setOutcome("failed");
    }
  };

  const status = outcome && (
    <StatusPill
      tone={outcome === "running" || outcome === "ok" ? "success" : outcome === "failed" ? "danger" : "neutral"}
      pulse={outcome === "running"}
    >
      {t(`status.${outcome}`)}
    </StatusPill>
  );

  return (
    <div className="ping-pong">
      <form
        className="ping-pong__toolbar"
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          if (!running) ping();
        }}
      >
        <TextField
          label={t("host")}
          value={host}
          placeholder={t("placeholder")}
          error={hostError}
          disabled={running}
          autoFocus
          autoCapitalize="off"
          onChange={(value) => {
            setHost(value);
            setHostError(null);
          }}
        />
        <NumericUpDown label={t("count")} value={count} onChange={setCount} min={1} max={MAX_COUNT} disabled={running} />
        <div className="ping-pong__action">
          {/* Distinct keys: if React reused one <button>, clicking Stop could turn it into the
              submit button mid-click and immediately start a new ping. */}
          {running ? (
            <Button key="stop" variant="danger" icon={StopCircle} onClick={() => invoke("ping_stop")}>
              {t("stop")}
            </Button>
          ) : (
            <Button key="ping" type="submit" variant="primary" icon={PingIcon}>
              {t("ping")}
            </Button>
          )}
        </div>
      </form>

      <Console
        title={t("console")}
        lines={lines}
        readOnly
        busy={running}
        status={status}
        placeholder={t("consolePlaceholder")}
        onClear={() => {
          setLines([]);
          setOutcome(null);
        }}
      />
    </div>
  );
}
