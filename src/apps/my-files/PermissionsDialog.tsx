import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { ShieldCheck } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { StateMessage } from "../../components/StateMessage";
import { StatusPill } from "../../components/StatusPill";
import { Switch } from "../../components/Switch";
import { Dialog } from "../../components/dialogs/Dialog";
import type { TreeEntry } from "../../components/Tree";
import { describeError } from "../../lib/errors";
import { files, type FilePermissions } from "./api";
import "./PermissionsDialog.css";

interface PermissionsDialogProps {
  home: string;
  entry: TreeEntry;
  onClose: () => void;
}

type Key = keyof FilePermissions;

/**
 * Read / write (and execute on Linux/macOS) for one file. "Current access" is
 * measured by the backend by actually opening the file, so after Apply it shows
 * whether the change really took effect.
 */
export function PermissionsDialog({ home, entry, onClose }: PermissionsDialogProps) {
  const { t } = useTranslation("my-files");
  const { t: tCommon } = useTranslation();
  const idBase = useId();
  const [current, setCurrent] = useState<FilePermissions | null>(null);
  const [draft, setDraft] = useState<FilePermissions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);

  useEffect(() => {
    files
      .permissions(home, entry.id)
      .then((state) => {
        setCurrent(state);
        setDraft(state);
      })
      .catch((cause) => setError(describeError(cause)));
  }, [home, entry.id]);

  const changed =
    !!current && !!draft && (["read", "write", "execute"] as Key[]).some((key) => current[key] !== draft[key]);

  const apply = async () => {
    if (!draft) return;
    setApplying(true);
    setError(null);
    try {
      const result = await files.setPermissions(home, entry.id, draft);
      setCurrent(result);
      setDraft(result);
      setApplied(true);
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setApplying(false);
    }
  };

  const rows: { key: Key; label: string; hint: string }[] = [
    { key: "read", label: t("permissions.read"), hint: t("permissions.readHint") },
    { key: "write", label: t("permissions.write"), hint: t("permissions.writeHint") },
    ...(draft?.execute !== null && draft?.execute !== undefined
      ? [{ key: "execute" as Key, label: t("permissions.execute"), hint: t("permissions.executeHint") }]
      : []),
  ];

  return (
    <Dialog
      open
      title={t("permissions.title")}
      icon={ShieldCheck}
      onClose={() => !applying && onClose()}
      actions={
        <>
          <Button onClick={onClose} disabled={applying}>
            {applied ? tCommon("about.close") : tCommon("dialog.cancel")}
          </Button>
          <Button variant="primary" loading={applying} disabled={!changed} onClick={apply}>
            {t("permissions.apply")}
          </Button>
        </>
      }
    >
      <p>{t("permissions.message", { name: entry.name })}</p>

      {!current && !error && <StateMessage kind="loading" />}

      {current && draft && (
        <>
          <div className="permissions__current">
            <span className="permissions__caption">{t("permissions.current")}</span>
            <StatusPill tone={current.read ? "success" : "danger"}>
              {current.read ? t("permissions.readable") : t("permissions.notReadable")}
            </StatusPill>
            <StatusPill tone={current.write ? "success" : "warning"}>
              {current.write ? t("permissions.writable") : t("permissions.readOnly")}
            </StatusPill>
            {current.execute !== null && (
              <StatusPill tone={current.execute ? "success" : "neutral"}>
                {current.execute ? t("permissions.executable") : t("permissions.notExecutable")}
              </StatusPill>
            )}
          </div>

          <ul className="permissions__list">
            {rows.map(({ key, label, hint }) => {
              const labelId = `${idBase}-${key}`;
              return (
                <li key={key} className="permissions__row">
                  <span className="permissions__text">
                    <span id={labelId} className="permissions__label">
                      {label}
                    </span>
                    <span className="permissions__hint">{hint}</span>
                  </span>
                  <Switch
                    checked={!!draft[key]}
                    labelledBy={labelId}
                    disabled={applying}
                    onChange={(value) => {
                      setDraft({ ...draft, [key]: value });
                      setApplied(false);
                    }}
                  />
                </li>
              );
            })}
          </ul>

          {applied && (
            <p className="permissions__applied" role="status">
              {t("permissions.applied")}
            </p>
          )}
        </>
      )}

      {error && (
        <p className="dialog-error" role="alert">
          {error}
        </p>
      )}
    </Dialog>
  );
}
