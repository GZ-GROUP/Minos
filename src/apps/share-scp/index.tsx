import { useCallback, useEffect, useId, useMemo, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { ArrowClockwise, DownloadSimple, LockKey, PlugsConnected, ShieldCheck, SignOut, UploadSimple } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { FileSelector, type SelectedFile } from "../../components/FileSelector";
import { StateMessage } from "../../components/StateMessage";
import { StatusPill } from "../../components/StatusPill";
import { Switch } from "../../components/Switch";
import { TextField } from "../../components/TextField";
import { Tooltip } from "../../components/Tooltip";
import { Tree, type TreeEntry } from "../../components/Tree";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { describeError, errorCode, errorDetail } from "../../lib/errors";
import "./ShareScp.css";

interface ConnectInfo {
  home: string;
  fingerprint: string;
  newHost: boolean;
}

interface RemoteListing {
  path: string;
  crumbs: { name: string; path: string }[];
  entries: { name: string; path: string; kind: "dir" | "file"; size: number; modifiedMs: number | null; hidden: boolean }[];
}

interface Session extends ConnectInfo {
  host: string;
  user: string;
}

/** Only non-secret fields are remembered; the password never is. */
const STORAGE_KEY = "minos.scp";

function loadSaved(): { host: string; port: string; user: string } {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return { host: saved.host ?? "", port: saved.port ?? "22", user: saved.user ?? "" };
  } catch {
    return { host: "", port: "22", user: "" };
  }
}

const REMOTE_ROOT = "/";

export default function ShareScp() {
  const { t } = useTranslation("share-scp");
  const { t: tCommon } = useTranslation();
  const { alert, confirm } = useDialogs();
  const hiddenLabelId = useId();

  const saved = useMemo(loadSaved, []);
  const [host, setHost] = useState(saved.host);
  const [port, setPort] = useState(saved.port);
  const [user, setUser] = useState(saved.user);
  const [password, setPassword] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);

  const [session, setSession] = useState<Session | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [listing, setListing] = useState<RemoteListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [localFile, setLocalFile] = useState<SelectedFile | null>(null);
  const [transferring, setTransferring] = useState<"upload" | "download" | null>(null);
  const requestId = useRef(0);

  // Close the SSH session when leaving the app.
  useEffect(
    () => () => {
      if (isTauri()) invoke("scp_disconnect").catch(() => {});
    },
    [],
  );

  const fail = (error: unknown) => {
    const detail = errorDetail(error);
    return alert({
      tone: "danger",
      title: tCommon("errors.title"),
      message: detail ? `${describeError(error)} (${detail})` : describeError(error),
    });
  };

  const load = useCallback(
    async (target: string) => {
      const id = ++requestId.current;
      setLoading(true);
      try {
        const result = await invoke<RemoteListing>("scp_list", { path: target, showHidden });
        if (id !== requestId.current) return;
        setListing(result);
        setListError(null);
      } catch (error) {
        if (id !== requestId.current) return;
        if (errorCode(error) === "not_connected") setSession(null);
        else setListError(describeError(error));
      } finally {
        if (id === requestId.current) setLoading(false);
      }
    },
    [showHidden],
  );

  useEffect(() => {
    if (session && path) load(path);
  }, [session, path, load]);

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const connect = async (event: FormEvent) => {
    event.preventDefault();
    const portNumber = Number(port);
    if (!host.trim() || !user.trim() || !password || !Number.isInteger(portNumber) || portNumber < 1 || portNumber > 65535) {
      setFormError(t("required"));
      return;
    }
    setConnecting(true);
    setFormError(null);
    try {
      const info = await invoke<ConnectInfo>("scp_connect", { host, port: portNumber, username: user, password });
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ host: host.trim(), port, user: user.trim() }));
      } catch {
        // Remembering the form is a convenience only.
      }
      setPassword("");
      setSession({ ...info, host: host.trim(), user: user.trim() });
      setSelected(null);
      setPath(info.home);
    } catch (error) {
      if (errorCode(error) === "host_key_changed") await fail(error);
      else setFormError(describeError(error));
    } finally {
      setConnecting(false);
    }
  };

  const disconnect = async () => {
    await invoke("scp_disconnect").catch(() => {});
    setSession(null);
    setListing(null);
    setPath(null);
    setLocalFile(null);
  };

  const upload = async (overwrite = false): Promise<void> => {
    if (!localFile || !listing) return;
    setTransferring("upload");
    try {
      const remote = await invoke<string>("scp_upload", {
        localPath: localFile.path,
        remoteDir: listing.path,
        overwrite,
      });
      await load(listing.path);
      setSelected(remote);
    } catch (error) {
      if (errorCode(error) === "exists" && !overwrite) {
        const choice = await confirm({
          tone: "danger",
          title: t("overwriteTitle", { name: localFile.name }),
          message: t("overwriteMessage"),
          options: [
            { id: "cancel", label: tCommon("dialog.cancel") },
            { id: "replace", label: t("replace"), variant: "danger" },
          ],
        });
        if (choice === "replace") return upload(true);
      } else {
        await fail(error);
      }
    } finally {
      setTransferring(null);
    }
  };

  const entries: TreeEntry[] | null =
    listing?.entries.map((entry) => ({
      id: entry.path,
      name: entry.name,
      kind: entry.kind === "dir" ? "folder" : "file",
      size: entry.size,
      modifiedMs: entry.modifiedMs,
      hidden: entry.hidden,
    })) ?? null;
  const selectedFile = entries?.find((entry) => entry.id === selected && entry.kind === "file") ?? null;

  const download = async () => {
    if (!selectedFile) return;
    try {
      const destination = await save({ defaultPath: selectedFile.name });
      if (!destination) return;
      setTransferring("download");
      await invoke("scp_download", { remotePath: selectedFile.id, localPath: destination });
      await alert({ tone: "success", title: t("download"), message: t("downloaded", { path: destination }) });
    } catch (error) {
      await fail(error);
    } finally {
      setTransferring(null);
    }
  };

  // ── Disconnected: connection form ───────────────────────
  if (!session) {
    return (
      <div className="share-scp share-scp--form">
        <form className="share-scp__form clay" onSubmit={connect} noValidate>
          <header className="share-scp__form-header">
            <span className="share-scp__form-icon" aria-hidden="true">
              <LockKey />
            </span>
            <h3>{t("connectTitle")}</h3>
          </header>
          <div className="share-scp__row">
            <TextField label={t("host")} value={host} onChange={setHost} placeholder="192.168.0.10" autoFocus />
            <TextField
              className="share-scp__port"
              label={t("port")}
              value={port}
              onChange={(value) => setPort(value.replace(/\D/g, ""))}
              inputMode="numeric"
            />
          </div>
          <TextField label={t("user")} value={user} onChange={setUser} autoCapitalize="off" />
          <TextField
            label={t("password")}
            type="password"
            value={password}
            onChange={setPassword}
            error={formError}
          />
          <Button type="submit" variant="primary" icon={PlugsConnected} loading={connecting}>
            {t("connect")}
          </Button>
          <p className="share-scp__security">
            <ShieldCheck aria-hidden="true" />
            {t("security")}
          </p>
        </form>
      </div>
    );
  }

  // ── Connected: transfers + remote tree ──────────────────
  return (
    <div className="share-scp">
      <div className="share-scp__bar">
        <Tooltip label={t("fingerprint", { fingerprint: session.fingerprint })}>
          <span tabIndex={0} className="share-scp__identity">
            <StatusPill tone="success" size="md">
              {t("connectedAs", { user: session.user, host: session.host })}
            </StatusPill>
          </span>
        </Tooltip>
        {session.newHost && <p className="share-scp__new-host">{t("newHost", { fingerprint: session.fingerprint })}</p>}
        <Button icon={SignOut} onClick={disconnect}>
          {t("disconnect")}
        </Button>
      </div>

      <div className="share-scp__transfer">
        <div className="share-scp__local">
          <FileSelector size="sm" file={localFile} onChange={setLocalFile} placeholder={t("chooseLocal")} />
          <Button
            variant="primary"
            icon={UploadSimple}
            loading={transferring === "upload"}
            disabled={!localFile || !listing || transferring !== null}
            onClick={() => upload()}
          >
            {t("upload")}
          </Button>
        </div>
        <Tooltip label={selectedFile ? selectedFile.name : t("selectFile")}>
          <Button
            icon={DownloadSimple}
            loading={transferring === "download"}
            disabled={!selectedFile || transferring !== null}
            onClick={download}
          >
            {t("download")}
          </Button>
        </Tooltip>
      </div>

      <Tree
        label={t("remoteTree")}
        home={REMOTE_ROOT}
        crumbs={listing?.crumbs ?? null}
        entries={entries}
        loading={loading}
        error={listError}
        selected={selected}
        onSelect={setSelected}
        onNavigate={(target) => {
          setSelected(null);
          setPath(target);
        }}
        actions={
          <>
            <span id={hiddenLabelId} className="share-scp__switch-label">
              {t("showHidden")}
            </span>
            <Switch checked={showHidden} onChange={setShowHidden} labelledBy={hiddenLabelId} />
            <Button
              iconOnly
              size="sm"
              variant="ghost"
              icon={ArrowClockwise}
              label={tCommon("list.refresh")}
              loading={loading && !!listing}
              onClick={() => path && load(path)}
            />
          </>
        }
      />
    </div>
  );
}
