import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { ArrowDown, ArrowsMerge, ArrowUp, Plus } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { ConvertLayout } from "../../components/ConvertLayout";
import { FileSelector, pickFiles, type SelectedFile } from "../../components/FileSelector";
import { StateMessage } from "../../components/StateMessage";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { describeError, errorDetail } from "../../lib/errors";
import "./MergePdf.css";

const PDF = [{ name: "PDF", extensions: ["pdf"] }];

interface Item extends SelectedFile {
  /** Stable key: the same file may be added twice. */
  key: number;
}

let nextKey = 0;
const withKeys = (files: SelectedFile[]): Item[] => files.map((file) => ({ ...file, key: nextKey++ }));

export default function MergePdf() {
  const { t } = useTranslation("merge-pdf");
  const { t: tCommon } = useTranslation();
  const { alert } = useDialogs();
  const [items, setItems] = useState<Item[]>([]);
  const [result, setResult] = useState<SelectedFile | null>(null);
  const [busy, setBusy] = useState(false);

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const fail = (error: unknown) => {
    const detail = errorDetail(error);
    return alert({
      tone: "danger",
      title: tCommon("errors.title"),
      message: detail ? `${describeError(error)} (${detail})` : describeError(error),
    });
  };

  const update = (next: Item[]) => {
    setItems(next);
    setResult(null);
  };

  const add = async () => {
    try {
      const picked = await pickFiles({ multiple: true, filters: PDF });
      if (picked.length) update([...items, ...withKeys(picked)]);
    } catch (error) {
      await fail(error);
    }
  };

  const move = (index: number, offset: -1 | 1) => {
    const next = [...items];
    [next[index], next[index + offset]] = [next[index + offset], next[index]];
    update(next);
  };

  const merge = async () => {
    setBusy(true);
    setResult(null);
    try {
      setResult(await invoke<SelectedFile>("pdf_merge", { paths: items.map((item) => item.path) }));
    } catch (error) {
      await fail(error);
    } finally {
      setBusy(false);
    }
  };

  const list =
    items.length === 0 ? (
      <FileSelector
        size="lg"
        file={null}
        filters={PDF}
        placeholder={t("choose")}
        onChange={(file) => file && update(withKeys([file]))}
      />
    ) : (
      <div className="merge-pdf__list">
        <ol className="merge-pdf__items" aria-label={t("order")}>
          {items.map((item, index) => (
            <li key={item.key} className="merge-pdf__item">
              <span className="merge-pdf__index" aria-hidden="true">
                {index + 1}
              </span>
              <FileSelector
                size="md"
                file={item}
                filters={PDF}
                disabled={busy}
                onChange={(file) =>
                  update(
                    file
                      ? items.map((current, i) => (i === index ? { ...file, key: current.key } : current))
                      : items.filter((_, i) => i !== index),
                  )
                }
              />
              <span className="merge-pdf__move">
                <Button
                  iconOnly
                  size="sm"
                  variant="ghost"
                  icon={ArrowUp}
                  label={t("moveUp", { name: item.name })}
                  disabled={busy || index === 0}
                  onClick={() => move(index, -1)}
                />
                <Button
                  iconOnly
                  size="sm"
                  variant="ghost"
                  icon={ArrowDown}
                  label={t("moveDown", { name: item.name })}
                  disabled={busy || index === items.length - 1}
                  onClick={() => move(index, 1)}
                />
              </span>
            </li>
          ))}
        </ol>
        <Button icon={Plus} size="sm" disabled={busy} onClick={add}>
          {t("add")}
        </Button>
      </div>
    );

  return (
    <ConvertLayout
      input={list}
      output={<FileSelector size="lg" mode="download" file={result} filters={PDF} busy={busy} />}
      action={
        <Button variant="primary" icon={ArrowsMerge} loading={busy} disabled={items.length < 2} onClick={merge}>
          {items.length < 2 && items.length > 0 ? t("needTwo") : t("merge", { count: items.length })}
        </Button>
      }
    />
  );
}
