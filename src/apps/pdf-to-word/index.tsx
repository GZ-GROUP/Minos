import { useTranslation } from "react-i18next";
import { OfficeConverter } from "../_shared/OfficeConverter";

export default function PdfToWord() {
  const { t } = useTranslation("pdf-to-word");
  return (
    <OfficeConverter
      command="convert_pdf_to_word"
      inputFilters={[{ name: "PDF", extensions: ["pdf"] }]}
      outputFilters={[{ name: "Word", extensions: ["docx"] }]}
      inputPlaceholder={t("choose")}
      actionLabel={t("convert")}
    />
  );
}
