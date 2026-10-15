import { X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";

import { useHotkey } from "../lib/hotkeys";
import { IconButton } from "./Button";

type DrawerProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
};

export function Drawer({ open, onClose, title, actions, children }: DrawerProps) {
  const titleId = useId();
  const ref = useRef<HTMLElement>(null);
  useHotkey("Escape", onClose, { enabled: open, allowInInputs: true });

  useEffect(() => {
    if (open) ref.current?.focus({ preventScroll: true });
  }, [open]);

  if (!open) return null;
  return (
    <aside
      ref={ref}
      tabIndex={-1}
      aria-labelledby={titleId}
      className="fixed top-[calc(var(--spacing-topbar)+env(safe-area-inset-top,0px))] right-0 bottom-[calc(var(--spacing-statusbar)+env(safe-area-inset-bottom,0px))] z-50 flex w-[min(520px,100%)] flex-col border-l border-border-strong bg-surface shadow-[-16px_0_40px_rgba(0,0,0,0.35)] outline-none max-lg:top-auto max-lg:left-0 max-lg:h-[min(78vh,680px)] max-lg:w-full max-lg:rounded-t-xl max-lg:border-t max-lg:border-l-0"
    >
      <div className="flex items-center gap-2 border-b border-border py-2.5 pr-3 pl-4">
        <h2 id={titleId} className="min-w-0 flex-1 text-13 font-semibold">
          {title}
        </h2>
        {actions}
        <IconButton label="Close" onClick={onClose}>
          <X className="size-3.5" />
        </IconButton>
      </div>
      <div className="flex flex-1 flex-col gap-3.5 overflow-y-auto px-4 pt-3.5 pb-6">{children}</div>
    </aside>
  );
}
