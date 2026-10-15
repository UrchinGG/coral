import { LoaderCircle } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cx } from "../lib/cx";

export type ButtonVariant = "default" | "primary" | "danger" | "ghost";
export type ButtonSize = "md" | "sm";

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  pending?: boolean;
  icon?: ReactNode;
};

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  size?: ButtonSize;
};

const BASE =
  "inline-flex items-center justify-center gap-1.5 rounded-md border font-medium whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-60";

const VARIANTS: Record<ButtonVariant, string> = {
  default: "border-border-strong bg-surface-2 text-fg hover:border-border-hover hover:bg-overlay",
  primary: "border-accent bg-accent font-semibold text-accent-ink hover:bg-accent-hover",
  danger: "border-danger/35 bg-danger/13 text-danger hover:bg-danger/20",
  ghost: "border-transparent bg-transparent text-muted hover:bg-surface-2 hover:text-fg",
};

const SIZES: Record<ButtonSize, string> = {
  md: "h-7 px-2.5 text-12",
  sm: "h-6 px-2 text-11",
};

const ICON_SIZES: Record<ButtonSize, string> = {
  md: "size-8",
  sm: "size-[22px]",
};

export function Button({ variant = "default", size = "md", pending = false, icon, className, children, disabled, type = "button", ...rest }: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(BASE, VARIANTS[variant], SIZES[size], pending && "cursor-progress", className)}
      disabled={disabled || pending}
      aria-busy={pending || undefined}
      {...rest}
    >
      {pending ? <LoaderCircle className="size-3 animate-spin" /> : icon}
      {children}
    </button>
  );
}

export function IconButton({ label, size = "md", className, children, type = "button", ...rest }: IconButtonProps) {
  return (
    <button
      type={type}
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-md border border-transparent text-muted transition-colors hover:bg-surface-2 hover:text-fg",
        ICON_SIZES[size],
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
