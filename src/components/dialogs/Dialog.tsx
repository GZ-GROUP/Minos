import { useEffect, useId, useRef, type FormEvent, type ReactNode } from "react";
import type { Icon } from "@phosphor-icons/react";
import "./Dialog.css";

export type DialogTone = "default" | "danger" | "success";

interface DialogProps {
  open: boolean;
  /** Called on Esc, backdrop click or any cancel action. */
  onClose: () => void;
  title: string;
  icon?: Icon;
  tone?: DialogTone;
  /** Replaces the icon bubble, e.g. a logo. */
  media?: ReactNode;
  align?: "start" | "center";
  children?: ReactNode;
  actions?: ReactNode;
  /** Small print under the actions, separated by a divider (links, version…). */
  footer?: ReactNode;
  /** Wraps the content in a form so Enter submits it. */
  onSubmit?: (event: FormEvent<HTMLFormElement>) => void;
}

/**
 * Base for every modal in Minos. Uses the native <dialog> for focus trapping,
 * Esc handling, top-layer rendering and focus restoration on close.
 */
export function Dialog({
  open,
  onClose,
  title,
  icon: IconComponent,
  tone = "default",
  media,
  align = "start",
  children,
  actions,
  footer,
  onSubmit,
}: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  const content = (
    <>
      <header className="dialog__header">
        {media ??
          (IconComponent && (
            <span className="dialog__icon" aria-hidden="true">
              <IconComponent />
            </span>
          ))}
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>
      </header>
      {children && <div className="dialog__body">{children}</div>}
      {actions && <footer className="dialog__actions">{actions}</footer>}
      {footer && <div className="dialog__footer">{footer}</div>}
    </>
  );

  return (
    <dialog
      ref={ref}
      className="dialog clay"
      data-tone={tone}
      data-align={align}
      aria-labelledby={titleId}
      onCancel={(event) => {
        // Keep React in control of `open`; the parent decides how to close.
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        // Clicking the backdrop targets the dialog element itself.
        if (event.target === event.currentTarget) onClose();
      }}
    >
      {onSubmit ? (
        <form
          className="dialog__content"
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit(event);
          }}
        >
          {content}
        </form>
      ) : (
        <div className="dialog__content">{content}</div>
      )}
    </dialog>
  );
}
