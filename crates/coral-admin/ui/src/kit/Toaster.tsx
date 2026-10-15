import { X } from "lucide-react";

import { cx } from "../lib/cx";
import { IconButton } from "./Button";
import { dismissToast, useToasts, type ToastTone } from "./toast";
import { TopLayer } from "./TopLayer";

const DOT: Record<ToastTone, string> = {
  ok: "bg-ok",
  error: "bg-danger",
  pending: "bg-info animate-pulse",
};

export function Toaster() {
  const toasts = useToasts();
  if (toasts.length === 0) return null;
  return (
    <TopLayer
      aria-live="polite"
      className="fixed top-auto right-4 bottom-[calc(var(--spacing-statusbar)+12px+env(safe-area-inset-bottom,0px))] left-auto m-0 flex w-[min(380px,calc(100%-32px))] flex-col gap-2 overflow-visible border-0 bg-transparent p-0 text-fg"
    >
      {toasts.map((item) => (
        <div
          key={item.id}
          role={item.tone === "error" ? "alert" : "status"}
          className="flex animate-toast-in items-start gap-2.5 rounded-lg border border-border-strong bg-overlay px-3 py-2.5 text-12 shadow-[0_12px_32px_rgba(0,0,0,0.45)]"
        >
          <span className={cx("mt-[5px] size-[7px] shrink-0 rounded-full", DOT[item.tone])} />
          <div className="min-w-0 flex-1 leading-normal wrap-anywhere">
            {item.message}
            {item.action !== null && (
              <button
                type="button"
                className="ml-2 p-0 text-11 text-accent"
                onClick={() => {
                  item.action?.run();
                  dismissToast(item.id);
                }}
              >
                {item.action.label}
              </button>
            )}
          </div>
          <IconButton label="Dismiss" size="sm" onClick={() => dismissToast(item.id)}>
            <X className="size-3" />
          </IconButton>
        </div>
      ))}
    </TopLayer>
  );
}
