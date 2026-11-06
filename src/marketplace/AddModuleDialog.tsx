import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
import { FileZip, Plus } from "@phosphor-icons/react";
import { Button } from "../components/Button";
import { TextField } from "../components/TextField";
import { Dialog } from "../components/dialogs/Dialog";
import { ErrorNote } from "./ErrorNote";
import type { Marketplace } from "./useMarketplace";

interface AddModuleDialogProps {
  open: boolean;
  onClose: () => void;
  market: Marketplace;
}

/** Adds a module from a GitHub link or a local .zip. The backend does the validation. */
export function AddModuleDialog({ open, onClose, market }: AddModuleDialogProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState<"link" | "zip" | null>(null);
  const [error, setError] = useState<unknown>(null);

  useEffect(() => {
    if (!open) return;
    setUrl("");
    setError(null);
  }, [open]);

  const attempt = async (kind: "link" | "zip", action: () => Promise<void>) => {
    setBusy(kind);
    setError(null);
    try {
      await action();
      onClose();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(null);
    }
  };

  const addLink = () => {
    if (busy || !url.trim()) return;
    attempt("link", () => market.addRepository(url));
  };

  const addZip = async () => {
    if (busy) return;
    const path = await openFilePicker({
      multiple: false,
      directory: false,
      filters: [{ name: t("marketplace.add.zipFilter"), extensions: ["zip"] }],
    });
    if (typeof path === "string") attempt("zip", () => market.addZip(path));
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("marketplace.add.title")}
      icon={Plus}
      onSubmit={addLink}
      actions={
        <>
          <Button onClick={onClose}>{t("dialog.cancel")}</Button>
          <Button icon={FileZip} loading={busy === "zip"} disabled={busy !== null} onClick={addZip}>
            {t("marketplace.add.chooseZip")}
          </Button>
          <Button
            type="submit"
            variant="primary"
            loading={busy === "link"}
            disabled={busy !== null || !url.trim()}
          >
            {t("marketplace.add.submit")}
          </Button>
        </>
      }
    >
      <TextField
        label={t("marketplace.add.linkLabel")}
        value={url}
        onChange={setUrl}
        placeholder="https://github.com/owner/repository"
        hint={t("marketplace.add.linkHint")}
      />
      {error != null && <ErrorNote error={error} />}
    </Dialog>
  );
}
