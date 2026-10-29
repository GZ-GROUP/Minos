import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Lightning, type Icon } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { ConvertLayout } from "../../components/ConvertLayout";
import { FileSelector, type FileFilter, type SelectedFile } from "../../components/FileSelector";
import { StateMessage } from "../../components/StateMessage";
import { StatusPill } from "../../components/StatusPill";
import { useDialogs } from "../../components/dialogs/DialogProvider";
import { NOT_AVAILABLE, useCommand } from "../../hooks/useCommand";
import { describeError, errorDetail } from "../../lib/errors";
import "./OfficeConverter.css";

interface ConversionResult {
  file: SelectedFile;
  engine: "libreoffice" | "basic";
}

interface OfficeConverterProps {
  command: "convert_pdf_to_word" | "convert_word_to_pdf";
  inputFilters: FileFilter[];
  outputFilters: FileFilter[];
  inputPlaceholder: string;
  actionLabel: string;
  actionIcon?: Icon;
}

/** Shared UI for PDF → Word and Word → PDF: input, arrow, result and a convert button. */
export function OfficeConverter({
  command,
  inputFilters,
  outputFilters,
  inputPlaceholder,
  actionLabel,
  actionIcon = Lightning,
}: OfficeConverterProps) {
  const { t } = useTranslation();
  const { alert } = useDialogs();
  const engine = useCommand<{ libreoffice: boolean }>("convert_engine");
  const [input, setInput] = useState<SelectedFile | null>(null);
  const [result, setResult] = useState<ConversionResult | null>(null);
  const [busy, setBusy] = useState(false);

  if (!isTauri()) return <StateMessage kind="error" error={NOT_AVAILABLE} />;

  const convert = async () => {
    if (!input) return;
    setBusy(true);
    setResult(null);
    try {
      setResult(await invoke<ConversionResult>(command, { path: input.path }));
    } catch (error) {
      const detail = errorDetail(error);
      await alert({
        tone: "danger",
        title: t("errors.title"),
        message: detail ? `${describeError(error)} (${detail})` : describeError(error),
      });
    } finally {
      setBusy(false);
    }
  };

  const hasLibreOffice = engine.data?.libreoffice;
  const notice = engine.data && (
    <div className="office-converter__engine">
      <StatusPill tone={hasLibreOffice ? "success" : "warning"}>
        {hasLibreOffice ? t("convert.libreoffice") : t("convert.basic")}
      </StatusPill>
      <p className="office-converter__hint">{hasLibreOffice ? t("convert.libreofficeHint") : t("convert.basicHint")}</p>
    </div>
  );

  return (
    <ConvertLayout
      notice={notice}
      input={
        <FileSelector
          size="lg"
          file={input}
          filters={inputFilters}
          placeholder={inputPlaceholder}
          disabled={busy}
          onChange={(file) => {
            setInput(file);
            setResult(null);
          }}
        />
      }
      output={
        <>
          <FileSelector size="lg" mode="download" file={result?.file ?? null} filters={outputFilters} busy={busy} />
          {result?.engine === "basic" && hasLibreOffice && (
            <p className="office-converter__hint">{t("convert.usedBasic")}</p>
          )}
        </>
      }
      action={
        <Button variant="primary" icon={actionIcon} loading={busy} disabled={!input} onClick={convert}>
          {actionLabel}
        </Button>
      }
    />
  );
}
