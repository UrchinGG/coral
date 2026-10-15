import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import { cx } from "../lib/cx";
import { useHotkey } from "../lib/hotkeys";

type PopoverProps = {
  label: string;
  button: ReactNode;
  buttonClassName?: string;
  panelClassName?: string;
  align?: "start" | "end";
  children: (close: () => void) => ReactNode;
};

export function Popover({ label, button, buttonClassName, panelClassName, align = "end", children }: PopoverProps) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const close = () => setOpen(false);
  useHotkey("Escape", close, { enabled: open, allowInInputs: true });

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && rootRef.current?.contains(event.target)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  return (
    <div ref={rootRef} className="relative inline-flex">
      <button
        type="button"
        aria-label={label}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => setOpen(!open)}
        className={buttonClassName}
      >
        {button}
      </button>
      {open && (
        <div
          id={panelId}
          role="dialog"
          aria-label={label}
          className={cx(
            "absolute top-[calc(100%+6px)] z-[70] rounded-lg border border-border-strong bg-overlay shadow-[0_16px_40px_rgba(0,0,0,0.5)]",
            align === "end" ? "right-0" : "left-0",
            panelClassName,
          )}
        >
          {children(close)}
        </div>
      )}
    </div>
  );
}
