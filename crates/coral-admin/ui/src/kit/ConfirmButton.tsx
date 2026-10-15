import { useEffect, useState, type ReactNode } from "react";

import { Button, type ButtonVariant } from "./Button";

type ConfirmButtonProps = {
  children: ReactNode;
  confirmLabel?: string;
  onConfirm: () => void;
  pending?: boolean;
  disabled?: boolean;
  variant?: ButtonVariant;
  size?: "md" | "sm";
};

const CONFIRM_WINDOW_MS = 4_000;

export function ConfirmButton({ children, confirmLabel = "Confirm", onConfirm, pending = false, disabled = false, variant = "default", size = "md" }: ConfirmButtonProps) {
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const timer = window.setTimeout(() => setArmed(false), CONFIRM_WINDOW_MS);
    return () => window.clearTimeout(timer);
  }, [armed]);

  if (!armed) {
    return (
      <Button variant={variant} size={size} disabled={disabled} pending={pending} onClick={() => setArmed(true)}>
        {children}
      </Button>
    );
  }
  return (
    <span className="inline-flex items-center gap-1">
      <Button
        variant="danger"
        size={size}
        pending={pending}
        onClick={() => {
          setArmed(false);
          onConfirm();
        }}
      >
        {confirmLabel}
      </Button>
      <Button variant="ghost" size={size} onClick={() => setArmed(false)}>
        Cancel
      </Button>
    </span>
  );
}
