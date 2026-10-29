import { forwardRef, type ButtonHTMLAttributes } from "react";
import { CircleNotch, type Icon } from "@phosphor-icons/react";
import { Tooltip } from "./Tooltip";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: "sm" | "md";
  icon?: Icon;
  /** Shows a spinner instead of the icon and disables the button. */
  loading?: boolean;
  /**
   * Renders only the icon. `label` then becomes the accessible name and a tooltip,
   * so icon-only buttons are never unlabeled.
   */
  iconOnly?: boolean;
  label?: string;
}

/** Standard clay button used across Minos. */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    variant = "secondary",
    size = "md",
    icon: IconComponent,
    loading = false,
    iconOnly = false,
    label,
    className,
    children,
    disabled,
    type = "button",
    ...rest
  },
  ref,
) {
  const icon = loading ? (
    <CircleNotch className="button__spinner" weight="bold" aria-hidden="true" />
  ) : (
    IconComponent && <IconComponent weight="bold" aria-hidden="true" />
  );

  const button = (
    <button
      ref={ref}
      type={type}
      className={["button", `button--${variant}`, `button--${size}`, iconOnly && "button--icon-only", className]
        .filter(Boolean)
        .join(" ")}
      disabled={disabled || loading}
      aria-label={iconOnly ? label : undefined}
      aria-busy={loading || undefined}
      {...rest}
    >
      {icon}
      {!iconOnly && (children ?? label)}
    </button>
  );

  return iconOnly && label ? <Tooltip label={label}>{button}</Tooltip> : button;
});
