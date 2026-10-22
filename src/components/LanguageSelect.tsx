import { useTranslation } from "react-i18next";
import { SUPPORTED_LANGUAGES } from "../i18n";
import "./LanguageSelect.css";

interface LanguageSelectProps {
  labelledBy: string;
}

/** Segmented control (radio group) listing every supported language. */
export function LanguageSelect({ labelledBy }: LanguageSelectProps) {
  const { t, i18n } = useTranslation();
  const current = i18n.resolvedLanguage;

  return (
    <div className="language-select" role="radiogroup" aria-labelledby={labelledBy}>
      {SUPPORTED_LANGUAGES.map(({ code, label }) => (
        <button
          key={code}
          type="button"
          role="radio"
          aria-checked={current === code}
          aria-label={t(`languages.${code}`)}
          className="language-select__option"
          onClick={() => i18n.changeLanguage(code)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
