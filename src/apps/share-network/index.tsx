import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Broadcast, Check, Copy, StopCircle } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { FileSelector, type SelectedFile } from "../../components/FileSelector";
import { StateMessage } from "../../components/StateMessage";
import { StatusPill } from "../../components/StatusPill";
import { TextField } from "../../components/TextField";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { describeError, errorDetail } from "../../lib/errors";
import "./ShareNetwork.css";

interface ShareInfo {
  url: string;
  qrSvg: string;
  file: SelectedFile;
}

interface ShareStatus {
  active: boolean;
  downloads: number;
  info: ShareInfo | null;
}

const POLL_MS = 2000;

export default function ShareNetwork() {
  const { t } = useTranslation("share-network");
  const { t: tCommon } = useTranslation();
  const { alert } = useDialogs();
  const [file, setFile] = useState<SelectedFile | null>(null);
  const [share, setShare] = useState<ShareInfo | null>(null);
  const [downloads, setDownloads] = useState(0);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  // Restore a share that is still running from a previous visit, then poll its counter.
  const refreshStatus = useCallback(async () => {
    const status = await invoke<ShareStatus>("share_status");
    setShare(status.info);
    setDownloads(status.downloads);
    if (status.info) setFile(status.info.file);
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    refreshStatus().catch(() => {});
  }, [refreshStatus]);

  useEffect(() => {
    if (!share) return;
    const id = window.setInterval(() => {
      if (!document.hidden) refreshStatus().catch(() => {});
    }, POLL_MS);
    return () => window.clearInterval(id);
  }, [share, refreshStatus]);

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const fail = (error: unknown) => {
    const detail = errorDetail(error);
    return alert({
      tone: "danger",
      title: tCommon("errors.title"),
      message: detail ? `${describeError(error)} (${detail})` : describeError(error),
    });
  };

  const start = async () => {
    if (!file) return;
    setBusy(true);
    try {
      setShare(await invoke<ShareInfo>("share_start", { path: file.path }));
      setDownloads(0);
    } catch (error) {
      await fail(error);
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    try {
      await invoke("share_stop");
      setShare(null);
      setDownloads(0);
    } catch (error) {
      await fail(error);
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    if (!share) return;
    await navigator.clipboard.writeText(share.url);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="share-network">
      <div className="share-network__file">
        <FileSelector
          size="lg"
          file={file}
          placeholder={t("choose")}
          disabled={busy || !!share}
          onChange={setFile}
        />
        {share ? (
          <Button variant="danger" icon={StopCircle} loading={busy} onClick={stop}>
            {t("stop")}
          </Button>
        ) : (
          <Button variant="primary" icon={Broadcast} loading={busy} disabled={!file} onClick={start}>
            {t("start")}
          </Button>
        )}
      </div>

      <section className="share-network__panel clay" aria-live="polite">
        {share ? (
          <>
            <StatusPill tone="success" size="md" pulse>
              {t("sharing", { count: downloads })}
            </StatusPill>
            {/* QR keeps dark-on-white in both themes so phones can always scan it. */}
            <div className="share-network__qr" role="img" aria-label={t("qrLabel")} dangerouslySetInnerHTML={{ __html: share.qrSvg }} />
            <TextField
              className="share-network__url"
              label={t("link")}
              value={share.url}
              readOnly
              onChange={() => {}}
              onFocus={(event) => event.currentTarget.select()}
              trailing={
                <Button iconOnly size="sm" variant="ghost" icon={copied ? Check : Copy} label={t("copy")} onClick={copy} />
              }
            />
            <p className="share-network__hint">{t("hint")}</p>
          </>
        ) : (
          <div className="share-network__idle">
            <Broadcast aria-hidden="true" />
            <p>{t("idle")}</p>
          </div>
        )}
      </section>
    </div>
  );
}
