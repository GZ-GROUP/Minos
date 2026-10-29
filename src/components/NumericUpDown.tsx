import { useEffect, useId, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Minus, Plus, WarningCircle } from "@phosphor-icons/react";
import "./TextField.css";
import "./NumericUpDown.css";

interface NumericUpDownProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  hint?: string;
  className?: string;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/**
 * Integer input with − / + steppers. Typing is limited to digits; out-of-range
 * values show an error while typing and are clamped when the field loses focus.
 */
export function NumericUpDown({
  label,
  value,
  onChange,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
  step = 1,
  disabled = false,
  hint,
  className,
}: NumericUpDownProps) {
  const { t } = useTranslation();
  const id = useId();
  const messageId = `${id}-message`;
  // The text is kept separately so the field can be empty or out of range while typing.
  const [text, setText] = useState(String(value));

  useEffect(() => setText(String(value)), [value]);

  const parsed = text === "" ? null : Number(text);
  const error =
    parsed === null
      ? t("numeric.required")
      : parsed < min
        ? t("numeric.min", { min })
        : parsed > max
          ? t("numeric.max", { max })
          : null;

  const commit = (next: number) => {
    const clamped = clamp(next, min, max);
    setText(String(clamped));
    onChange(clamped);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const current = parsed ?? value;
    const keys: Record<string, number> = { ArrowUp: step, ArrowDown: -step, PageUp: step * 10, PageDown: -step * 10 };
    if (event.key in keys) {
      event.preventDefault();
      commit(current + keys[event.key]);
    } else if (event.key === "Home" && min > Number.MIN_SAFE_INTEGER) {
      event.preventDefault();
      commit(min);
    } else if (event.key === "End" && max < Number.MAX_SAFE_INTEGER) {
      event.preventDefault();
      commit(max);
    }
  };

  const message = error ?? hint;

  return (
    <div className={["text-field", "numeric-updown", className].filter(Boolean).join(" ")}>
      <label htmlFor={id} className="text-field__label">
        {label}
      </label>
      <div className="text-field__control numeric-updown__control" data-invalid={error ? true : undefined}>
        <button
          type="button"
          className="numeric-updown__step"
          aria-label={t("numeric.decrease")}
          tabIndex={-1}
          disabled={disabled || (parsed ?? value) <= min}
          onClick={() => commit((parsed ?? value) - step)}
        >
          <Minus weight="bold" aria-hidden="true" />
        </button>
        <input
          id={id}
          className="text-field__input numeric-updown__input"
          role="spinbutton"
          inputMode="numeric"
          autoComplete="off"
          value={text}
          disabled={disabled}
          aria-valuemin={min}
          aria-valuemax={max < Number.MAX_SAFE_INTEGER ? max : undefined}
          aria-valuenow={parsed ?? undefined}
          aria-invalid={error ? true : undefined}
          aria-describedby={message ? messageId : undefined}
          onChange={(event) => {
            const digits = event.target.value.replace(/\D/g, "");
            setText(digits);
            const next = digits === "" ? null : Number(digits);
            if (next !== null && next >= min && next <= max) onChange(next);
          }}
          onKeyDown={onKeyDown}
          onBlur={() => commit(parsed ?? value)}
        />
        <button
          type="button"
          className="numeric-updown__step"
          aria-label={t("numeric.increase")}
          tabIndex={-1}
          disabled={disabled || (parsed ?? value) >= max}
          onClick={() => commit((parsed ?? value) + step)}
        >
          <Plus weight="bold" aria-hidden="true" />
        </button>
      </div>
      {message && (
        <p id={messageId} className={error ? "text-field__error" : "text-field__hint"} role={error ? "alert" : undefined}>
          {error && <WarningCircle weight="bold" aria-hidden="true" />}
          {message}
        </p>
      )}
    </div>
  );
}
