import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open, save, type DialogFilter } from "@tauri-apps/plugin-dialog";
import { ArrowsClockwise, Check, CircleNotch, DownloadSimple, FileArrowUp, Hourglass, X } from "@phosphor-icons/react";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";
import { useDialogs } from "./dialogs/DialogProvider";
import { fileIcon } from "../lib/fileIcons";
import { formatBytes } from "../lib/format";
import { describeError, errorDetail } from "../lib/errors";
import "./FileSelector.css";

export interface SelectedFile {
  path: string;
  name: string;
  size: number;
}

export type FileFilter = DialogFilter;

/** Opens the system file picker and returns info about the chosen files. */
export async function pickFiles(options: { multiple?: boolean; filters?: FileFilter[] } = {}): Promise<SelectedFile[]> {
  const picked = await open({ multiple: options.multiple ?? false, directory: false, filters: options.filters });
  if (!picked) return [];
  const paths = Array.isArray(picked) ? picked : [picked];
  return Promise.all(paths.map((path) => invoke<SelectedFile>("file_info", { path })));
}

interface FileSelectorProps {
  size?: "sm" | "md" | "lg";
  /** "select": pick/change/remove a local file. "download": read-only result with a download button. */
  mode?: "select" | "download";
  file: SelectedFile | null;
  onChange?: (file: SelectedFile | null) => void;
  filters?: FileFilter[];
  /** Empty-state text. */
  placeholder?: string;
  /** Download mode: the result is being produced. */
  busy?: boolean;
  disabled?: boolean;
  /** Download mode: custom save logic (defaults to copying a generated file via `file_export`). */
  onDownload?: (file: SelectedFile, destination: string) => Promise<void>;
}

/**
 * A single file slot. Large: icon on top, name and size below. Medium: icon left,
 * name and size beside it. Small: just name and size.
 */
export function FileSelector({
  size = "lg",
  mode = "select",
  file,
  onChange,
  filters,
  placeholder,
  busy = false,
  disabled = false,
  onDownload,
}: FileSelectorProps) {
  const { t } = useTranslation();
  const { alert } = useDialogs();
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);

  const fail = (error: unknown) =>
    alert({
      tone: "danger",
      title: t("errors.title"),
      message: errorDetail(error) ? `${describeError(error)} (${errorDetail(error)})` : describeError(error),
    });

  const choose = async () => {
    try {
      const [picked] = await pickFiles({ filters });
      if (picked) onChange?.(picked);
    } catch (error) {
      await fail(error);
    }
  };

  const download = async () => {
    if (!file) return;
    try {
      const destination = await save({ defaultPath: file.name, filters });
      if (!destination) return;
      setSaving(true);
      if (onDownload) await onDownload(file, destination);
      else await invoke("file_export", { source: file.path, destination });
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
    } catch (error) {
      await fail(error);
    } finally {
      setSaving(false);
    }
  };

  // ── Empty states ────────────────────────────────────────
  if (!file) {
    if (mode === "select") {
      return (
        <button
          type="button"
          className={`file-selector file-selector--${size} file-selector--empty`}
          disabled={disabled}
          onClick={choose}
        >
          <span className="file-selector__icon" aria-hidden="true">
            <FileArrowUp />
          </span>
          <span className="file-selector__text">
            <span className="file-selector__name">{placeholder ?? t("fileSelector.choose")}</span>
            {size !== "sm" && <span className="file-selector__meta">{t("fileSelector.browse")}</span>}
          </span>
        </button>
      );
    }
    return (
      <div
        className={`file-selector file-selector--${size} file-selector--pending`}
        role="status"
        aria-busy={busy || undefined}
      >
        <span className="file-selector__icon" aria-hidden="true">
          {busy ? <CircleNotch className="file-selector__spinner" /> : <Hourglass />}
        </span>
        <span className="file-selector__text">
          <span className="file-selector__name">
            {busy ? t("fileSelector.working") : (placeholder ?? t("fileSelector.result"))}
          </span>
        </span>
      </div>
    );
  }

  // ── File present ────────────────────────────────────────
  const Icon = fileIcon(file.name, "file");
  const actions =
    mode === "select" ? (
      <>
        <Button
          iconOnly
          size="sm"
          variant="ghost"
          icon={ArrowsClockwise}
          label={t("fileSelector.change")}
          disabled={disabled}
          onClick={choose}
        />
        <Button
          iconOnly
          size="sm"
          variant="ghost"
          icon={X}
          label={t("fileSelector.remove")}
          disabled={disabled}
          onClick={() => onChange?.(null)}
        />
      </>
    ) : (
      <Button
        size="sm"
        variant="primary"
        icon={saved ? Check : DownloadSimple}
        loading={saving}
        disabled={disabled}
        onClick={download}
      >
        {saved ? t("fileSelector.saved") : t("fileSelector.download")}
      </Button>
    );

  return (
    <div className={`file-selector file-selector--${size} file-selector--filled`} data-mode={mode}>
      {size !== "sm" && (
        <span className="file-selector__icon" aria-hidden="true">
          <Icon />
        </span>
      )}
      <span className="file-selector__text">
        <Tooltip label={file.name} whenTruncated className="file-selector__name-tip">
          <span className="file-selector__name" data-truncate>
            {file.name}
          </span>
        </Tooltip>
        <span className="file-selector__meta">{formatBytes(file.size)}</span>
      </span>
      <span className="file-selector__actions">{actions}</span>
    </div>
  );
}
