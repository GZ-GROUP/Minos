import { forwardRef, useId, type InputHTMLAttributes, type ReactNode } from "react";
import { WarningCircle } from "@phosphor-icons/react";
import "./TextField.css";

interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "size"> {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** Helper text under the field (replaced by `error` when present). */
  hint?: ReactNode;
  error?: string | null;
  /** Content inside the field, after the input (e.g. a copy button). */
  trailing?: ReactNode;
}

/** Labeled clay input with hint and inline error, wired for screen readers. */
export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(function TextField(
  { label, value, onChange, hint, error, trailing, id, className, ...rest },
  ref,
) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const messageId = `${inputId}-message`;
  const message = error ?? hint;

  return (
    <div className={["text-field", className].filter(Boolean).join(" ")}>
      <label htmlFor={inputId} className="text-field__label">
        {label}
      </label>
      <div className="text-field__control" data-invalid={error ? true : undefined}>
        <input
          ref={ref}
          id={inputId}
          className="text-field__input"
          value={value}
          autoComplete="off"
          spellCheck={false}
          aria-invalid={error ? true : undefined}
          aria-describedby={message ? messageId : undefined}
          onChange={(event) => onChange(event.target.value)}
          {...rest}
        />
        {trailing}
      </div>
      {message && (
        <p id={messageId} className={error ? "text-field__error" : "text-field__hint"} role={error ? "alert" : undefined}>
          {error && <WarningCircle weight="bold" aria-hidden="true" />}
          {message}
        </p>
      )}
    </div>
  );
});
