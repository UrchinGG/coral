import { useState } from "react";

type ConfirmButtonProps = {
  label: string;
  confirmLabel?: string;
  onConfirm: () => void;
  pending?: boolean;
  tone?: "default" | "danger";
  disabled?: boolean;
};

export function ConfirmButton({
  label,
  confirmLabel = "Confirm",
  onConfirm,
  pending = false,
  tone = "default",
  disabled = false,
}: ConfirmButtonProps) {
  const [confirming, setConfirming] = useState(false);

  const toneClasses =
    tone === "danger"
      ? "border-danger/40 text-danger hover:bg-danger/10"
      : "border-white/10 text-gray-200 hover:bg-white/10";

  if (confirming) {
    return (
      <span className="inline-flex items-center gap-1">
        <button
          disabled={pending}
          onClick={() => {
            onConfirm();
            setConfirming(false);
          }}
          className="rounded border border-danger/40 bg-danger/10 px-2 py-1 text-xs text-danger disabled:opacity-50"
        >
          {pending ? "…" : confirmLabel}
        </button>
        <button
          onClick={() => setConfirming(false)}
          className="rounded border border-white/10 px-2 py-1 text-xs text-gray-400 hover:bg-white/10"
        >
          Cancel
        </button>
      </span>
    );
  }

  return (
    <button
      disabled={disabled || pending}
      onClick={() => setConfirming(true)}
      className={`rounded border px-2 py-1 text-xs disabled:opacity-40 ${toneClasses}`}
    >
      {label}
    </button>
  );
}
