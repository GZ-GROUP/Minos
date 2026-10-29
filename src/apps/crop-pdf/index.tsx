import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Scissors } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { ConvertLayout } from "../../components/ConvertLayout";
import { FileSelector, type SelectedFile } from "../../components/FileSelector";
import { StateMessage } from "../../components/StateMessage";
import { TextField } from "../../components/TextField";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { NOT_AVAILABLE } from "../../hooks/useCommand";
import { describeError, errorDetail } from "../../lib/errors";

const PDF = [{ name: "PDF", extensions: ["pdf"] }];

type PatternResult = { pages: number[] } | { error: "invalid" } | { error: "outOfRange"; page: number };

/** Mirrors `parse_pages` in src-tauri/src/apps/pdf_tools.rs so mistakes show while typing. */
function parsePattern(pattern: string, total: number | null): PatternResult {
  const pages = new Set<number>();
  for (const part of pattern.split(/[,;]/).map((p) => p.trim()).filter(Boolean)) {
    const match = /^(\d+)\s*(?:-\s*(\d+))?$/.exec(part);
    if (!match) return { error: "invalid" };
    const start = Number(match[1]);
    const end = Number(match[2] ?? match[1]);
    if (start === 0 || start > end) return { error: "invalid" };
    if (total !== null && end > total) return { error: "outOfRange", page: end };
    for (let page = start; page <= end; page++) pages.add(page);
  }
  return pages.size ? { pages: [...pages] } : { error: "invalid" };
}

export default function CropPdf() {
  const { t } = useTranslation("crop-pdf");
  const { t: tCommon } = useTranslation();
  const { alert } = useDialogs();
  const [input, setInput] = useState<SelectedFile | null>(null);
  const [pageCount, setPageCount] = useState<number | null>(null);
  const [pattern, setPattern] = useState("");
  const [result, setResult] = useState<SelectedFile | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = (error: unknown) => {
    const detail = errorDetail(error);
    return alert({
      tone: "danger",
      title: tCommon("errors.title"),
      message: detail ? `${describeError(error)} (${detail})` : describeError(error),
    });
  };

  // Page count drives the hint and the range check.
  useEffect(() => {
    setPageCount(null);
    if (!input) return;
    invoke<{ pages: number }>("pdf_info", { path: input.path })
      .then((info) => setPageCount(info.pages))
      .catch((error) => {
        setInput(null);
        fail(error);
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [input]);

  const parsed = useMemo(() => (pattern.trim() ? parsePattern(pattern, pageCount) : null), [pattern, pageCount]);
  const error =
    parsed && "error" in parsed
      ? parsed.error === "outOfRange"
        ? t("outOfRange", { page: parsed.page, total: pageCount })
        : t("invalid")
      : null;
  const hint =
    parsed && "pages" in parsed
      ? t("selected", { count: parsed.pages.length })
      : pageCount !== null
        ? t("hintPages", { count: pageCount })
        : t("hintEmpty");

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const crop = async () => {
    if (!input || !parsed || "error" in parsed) return;
    setBusy(true);
    setResult(null);
    try {
      setResult(await invoke<SelectedFile>("pdf_crop", { path: input.path, pattern }));
    } catch (cause) {
      await fail(cause);
    } finally {
      setBusy(false);
    }
  };

  return (
    <ConvertLayout
      input={
        <FileSelector
          size="lg"
          file={input}
          filters={PDF}
          placeholder={t("choose")}
          disabled={busy}
          onChange={(file) => {
            setInput(file);
            setResult(null);
          }}
        />
      }
      output={<FileSelector size="lg" mode="download" file={result} filters={PDF} busy={busy} />}
      controls={
        <TextField
          label={t("pages")}
          value={pattern}
          placeholder={t("placeholder")}
          hint={hint}
          error={error}
          disabled={busy}
          inputMode="numeric"
          onChange={(value) => {
            setPattern(value);
            setResult(null);
          }}
          onKeyDown={(event) => event.key === "Enter" && crop()}
        />
      }
      action={
        <Button
          variant="primary"
          icon={Scissors}
          loading={busy}
          disabled={!input || pageCount === null || !parsed || "error" in parsed}
          onClick={crop}
        >
          {t("crop")}
        </Button>
      }
    />
  );
}
