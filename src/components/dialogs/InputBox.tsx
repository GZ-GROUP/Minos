import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { PencilSimple, WarningCircle, type Icon } from "@phosphor-icons/react";
import { Button } from "../Button";
import { Dialog } from "./Dialog";

export type FormValues = Record<string, string>;

export interface PromptField {
  name: string;
  label: string;
  defaultValue?: string;
  placeholder?: string;
  type?: "text" | "password" | "number";
  /** Pre-select the text before the last dot (rename "notes.txt" → selects "notes"). */
  selectBasename?: boolean;
}

export interface PromptOptions {
  title: string;
  message?: ReactNode;
  icon?: Icon;
  fields: PromptField[];
  confirmLabel?: string;
  cancelLabel?: string;
  /** Synchronous check; return an error message to block submission. */
  validate?: (values: FormValues) => string | null;
  /**
   * Runs on submit while the box shows a spinner. Return an error message to keep
   * the box open and show it (e.g. "already exists"); return nothing to close.
   */
  onSubmit?: (values: FormValues) => Promise<string | void | null> | string | void | null;
}

interface InputBoxProps {
  options: PromptOptions;
  onResolve: (values: FormValues | null) => void;
}

/** Prompt / simple form dialog. One or more text fields, inline error, async submit. */
export function InputBox({ options, onResolve }: InputBoxProps) {
  const { t } = useTranslation();
  const { title, message, icon = PencilSimple, fields, confirmLabel, cancelLabel, validate, onSubmit } = options;
  const [values, setValues] = useState<FormValues>(() =>
    Object.fromEntries(fields.map((field) => [field.name, field.defaultValue ?? ""])),
  );
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const firstInput = useRef<HTMLInputElement>(null);
  const baseId = useId();
  const errorId = `${baseId}-error`;

  // Focus + selection once the dialog is shown (showModal moves focus first).
  useEffect(() => {
    const input = firstInput.current;
    if (!input) return;
    const frame = requestAnimationFrame(() => {
      input.focus();
      const dot = input.value.lastIndexOf(".");
      if (fields[0]?.selectBasename && dot > 0) input.setSelectionRange(0, dot);
      else input.select();
    });
    return () => cancelAnimationFrame(frame);
  }, [fields]);

  const submit = async () => {
    if (submitting) return;
    const problem = validate?.(values) ?? null;
    if (problem) {
      setError(problem);
      firstInput.current?.focus();
      return;
    }
    setSubmitting(true);
    try {
      const result = await onSubmit?.(values);
      if (result) {
        setError(result);
        firstInput.current?.focus();
        return;
      }
      onResolve(values);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      open
      title={title}
      icon={icon}
      onClose={() => !submitting && onResolve(null)}
      onSubmit={submit}
      actions={
        <>
          <Button onClick={() => onResolve(null)} disabled={submitting}>
            {cancelLabel ?? t("dialog.cancel")}
          </Button>
          <Button type="submit" variant="primary" loading={submitting}>
            {confirmLabel ?? t("dialog.ok")}
          </Button>
        </>
      }
    >
      {message && <p>{message}</p>}
      {fields.map((field, index) => {
        const id = `${baseId}-${field.name}`;
        return (
          <div key={field.name} className="dialog-field">
            <label htmlFor={id} className="dialog-field__label">
              {field.label}
            </label>
            <input
              ref={index === 0 ? firstInput : undefined}
              id={id}
              className="dialog-field__input"
              type={field.type ?? "text"}
              value={values[field.name]}
              placeholder={field.placeholder}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : undefined}
              onChange={(event) => {
                setValues((current) => ({ ...current, [field.name]: event.target.value }));
                setError(null);
              }}
            />
          </div>
        );
      })}
      {error && (
        <p id={errorId} className="dialog-error" role="alert">
          <WarningCircle weight="bold" aria-hidden="true" />
          {error}
        </p>
      )}
    </Dialog>
  );
}
