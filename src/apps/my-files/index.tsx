import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { ArrowClockwise, FilePlus, FolderPlus, PencilSimple, ShieldCheck, Trash } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { StateMessage } from "../../components/StateMessage";
import { Switch } from "../../components/Switch";
import { Tree, type TreeAction, type TreeEntry } from "../../components/Tree";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import type { FormValues } from "../../components/dialogs/InputBox";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { errorCode, files, validateName, type Listing } from "./api";
import { PermissionsDialog } from "./PermissionsDialog";
import "./MyFiles.css";

export default function MyFiles() {
  const { t } = useTranslation("my-files");
  const { t: tCommon } = useTranslation();
  const { prompt, confirm, alert } = useDialogs();
  const hiddenLabelId = useId();

  const [home, setHome] = useState<string | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [listing, setListing] = useState<Listing | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [showHidden, setShowHidden] = useState(false);
  const [permissionsFor, setPermissionsFor] = useState<TreeEntry | null>(null);
  const requestId = useRef(0);

  const describe = useCallback(
    (cause: unknown) => t(`errors.${errorCode(cause)}`, { defaultValue: t("errors.io") }),
    [t],
  );

  // Home is the user's folder; the Tree never navigates above it.
  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    files
      .home()
      .then((folder) => {
        setHome(folder);
        setPath(folder);
      })
      .catch((cause) => {
        setError(describe(cause));
        setLoading(false);
      });
  }, [describe]);

  const load = useCallback(
    async (target: string) => {
      if (!home) return;
      const id = ++requestId.current;
      setLoading(true);
      try {
        const result = await files.list(home, target, showHidden);
        if (id !== requestId.current) return; // a newer navigation won
        setListing(result);
        setError(null);
      } catch (cause) {
        if (id !== requestId.current) return;
        // A folder that vanished or is out of bounds sends the user back home.
        const code = errorCode(cause);
        if ((code === "not_found" || code === "outside_home") && target !== home) setPath(home);
        else setError(describe(cause));
      } finally {
        if (id === requestId.current) setLoading(false);
      }
    },
    [home, showHidden, describe],
  );

  useEffect(() => {
    if (path) load(path);
  }, [path, load]);

  const reload = useCallback(() => (path ? load(path) : Promise.resolve()), [path, load]);

  const navigate = (target: string) => {
    setSelected(null);
    setPath(target);
  };

  const entries = useMemo<TreeEntry[] | null>(
    () =>
      listing?.entries.map((entry) => ({
        id: entry.path,
        name: entry.name,
        kind: entry.kind === "dir" ? "folder" : "file",
        size: entry.size,
        modifiedMs: entry.modifiedMs,
        hidden: entry.hidden,
      })) ?? null,
    [listing],
  );
  const selectedEntry = entries?.find((entry) => entry.id === selected) ?? null;

  const validate = (values: FormValues) => {
    const key = validateName(values.name);
    return key ? t(key) : null;
  };

  const create = async (kind: "file" | "folder") => {
    if (!home || !path) return;
    const prefix = kind === "file" ? "newFile" : "newFolder";
    await prompt({
      title: t(`${prefix}.title`),
      icon: kind === "file" ? FilePlus : FolderPlus,
      fields: [{ name: "name", label: t("fields.name"), defaultValue: t(`${prefix}.default`), selectBasename: true }],
      confirmLabel: t("actions.create"),
      validate,
      onSubmit: async ({ name }) => {
        try {
          const created = await (kind === "file" ? files.createFile : files.createDir)(home, path, name);
          await reload();
          setSelected(created);
        } catch (cause) {
          return describe(cause);
        }
      },
    });
  };

  const rename = async (entry: TreeEntry) => {
    if (!home) return;
    await prompt({
      title: t("rename.title"),
      message: t("rename.message", { name: entry.name }),
      icon: PencilSimple,
      fields: [
        { name: "name", label: t("fields.name"), defaultValue: entry.name, selectBasename: entry.kind === "file" },
      ],
      confirmLabel: t("actions.rename"),
      validate,
      onSubmit: async ({ name }) => {
        if (name === entry.name) return;
        try {
          const renamed = await files.rename(home, entry.id, name);
          await reload();
          setSelected(renamed);
        } catch (cause) {
          return describe(cause);
        }
      },
    });
  };

  const remove = async (entry: TreeEntry) => {
    if (!home) return;
    const choice = await confirm({
      tone: "danger",
      icon: Trash,
      title: t("delete.title", { name: entry.name }),
      message: t(entry.kind === "folder" ? "delete.folderMessage" : "delete.fileMessage", { name: entry.name }),
      options: [
        { id: "cancel", label: tCommon("dialog.cancel") },
        { id: "delete", label: t("actions.delete"), variant: "danger" },
      ],
    });
    if (choice !== "delete") return;
    try {
      await files.remove(home, entry.id);
      setSelected(null);
      await reload();
    } catch (cause) {
      await alert({ tone: "danger", title: t("errors.title"), message: describe(cause) });
    }
  };

  const onAction = (action: TreeAction, entry: TreeEntry) => (action === "rename" ? rename(entry) : remove(entry));

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  return (
    <div className="my-files">
      <div className="my-files__toolbar" role="toolbar" aria-label={t("name")}>
        <div className="my-files__group">
          <Button icon={FilePlus} disabled={!path} onClick={() => create("file")}>
            {t("actions.newFile")}
          </Button>
          <Button icon={FolderPlus} disabled={!path} onClick={() => create("folder")}>
            {t("actions.newFolder")}
          </Button>
        </div>
        <div className="my-files__group">
          <Button icon={PencilSimple} disabled={!selectedEntry} onClick={() => selectedEntry && rename(selectedEntry)}>
            {t("actions.rename")}
          </Button>
          <Button
            icon={Trash}
            className="my-files__delete"
            disabled={!selectedEntry}
            onClick={() => selectedEntry && remove(selectedEntry)}
          >
            {t("actions.delete")}
          </Button>
          {/* Permissions apply to files only (see files_set_permissions). */}
          <Button
            icon={ShieldCheck}
            disabled={selectedEntry?.kind !== "file"}
            title={selectedEntry?.kind === "folder" ? t("permissions.filesOnly") : undefined}
            onClick={() => selectedEntry && setPermissionsFor(selectedEntry)}
          >
            {t("actions.permissions")}
          </Button>
        </div>
      </div>

      {home && permissionsFor && (
        <PermissionsDialog home={home} entry={permissionsFor} onClose={() => setPermissionsFor(null)} />
      )}

      {home && (
        <Tree
          label={t("name")}
          home={home}
          crumbs={listing?.crumbs ?? null}
          entries={entries}
          loading={loading}
          error={error}
          selected={selected}
          onSelect={setSelected}
          onNavigate={navigate}
          onAction={onAction}
          actions={
            <>
              <span id={hiddenLabelId} className="my-files__switch-label">
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
                onClick={reload}
              />
            </>
          }
        />
      )}
      {!home && loading && <StateMessage kind="loading" />}
      {!home && error && <StateMessage kind="error" message={error} />}
    </div>
  );
}
