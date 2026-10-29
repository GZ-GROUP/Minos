import { useTranslation } from "react-i18next";
import { OfficeConverter } from "../_shared/OfficeConverter";

export default function WordToPdf() {
  const { t } = useTranslation("word-to-pdf");
  return (
    <OfficeConverter
      command="convert_word_to_pdf"
      inputFilters={[{ name: "Word", extensions: ["docx", "doc", "odt", "rtf"] }]}
      outputFilters={[{ name: "PDF", extensions: ["pdf"] }]}
      inputPlaceholder={t("choose")}
      actionLabel={t("convert")}
    />
  );
}
