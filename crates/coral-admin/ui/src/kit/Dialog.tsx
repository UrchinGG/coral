import { useId, useState, type FormEvent, type ReactNode } from "react";

import { cx } from "../lib/cx";
import { Button } from "./Button";
import { Input } from "./Input";
import { useModalDialog } from "./modal-stack";

type DialogProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  width?: "md" | "lg";
  className?: string;
};

type ConfirmDialogProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  children?: ReactNode;
  confirmLabel: string;
  tone?: "primary" | "danger";
  expect?: string;
  onConfirm: () => Promise<void>;
};

const WIDTHS = { md: "w-[min(480px,calc(100%-32px))]", lg: "w-[min(680px,calc(100%-32px))]" } as const;

export function Dialog({ open, onClose, title, children, actions, width = "md", className }: DialogProps) {
  const ref = useModalDialog(open);
  const titleId = useId();
  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
      className={cx(
        "m-auto mt-[12vh] max-h-[calc(100dvh-24vh)] rounded-[10px] border border-border-strong bg-overlay p-0 text-fg shadow-[0_24px_64px_rgba(0,0,0,0.55)] max-md:mt-4",
        WIDTHS[width],
        className,
      )}
    >
      {open && (
        <div className="flex flex-col gap-3.5 p-[18px]">
          <h2 id={titleId} className="text-15 font-semibold">
            {title}
          </h2>
          {children !== undefined && <div className="flex flex-col gap-3 leading-[1.55] text-muted">{children}</div>}
          {actions !== undefined && <div className="flex flex-wrap justify-end gap-2">{actions}</div>}
        </div>
      )}
    </dialog>
  );
}

export function ConfirmDialog({ open, onClose, title, children, confirmLabel, tone = "primary", expect, onConfirm }: ConfirmDialogProps) {
  const [typed, setTyped] = useState("");
  const formId = useId();
  const confirmable = expect === undefined || typed.trim() === expect;
  const finish = () => {
    setTyped("");
    onClose();
  };
  const confirmation = useConfirmation(onConfirm, finish);
  const dismiss = () => {
    confirmation.clearError();
    finish();
  };

  return (
    <Dialog
      open={open}
      onClose={dismiss}
      title={title}
      actions={
        <>
          <Button variant="ghost" onClick={dismiss}>
            Cancel
          </Button>
          <Button type="submit" form={formId} variant={tone === "danger" ? "danger" : "primary"} disabled={!confirmable} pending={confirmation.pending}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
      <form id={formId} onSubmit={(event) => confirmation.submit(event, confirmable)} className="flex flex-col gap-2">
        {expect !== undefined && <TypeToConfirm expect={expect} typed={typed} onTypedChange={setTyped} />}
        {confirmation.error !== null && (
          <span role="alert" className="text-11 text-danger">
            {confirmation.error}
          </span>
        )}
      </form>
    </Dialog>
  );
}

function TypeToConfirm({ expect, typed, onTypedChange }: { expect: string; typed: string; onTypedChange: (typed: string) => void }) {
  return (
    <Input
      mono
      label={
        <>
          Type <span className="font-mono tracking-normal text-fg normal-case">{expect}</span> to confirm
        </>
      }
      value={typed}
      onChange={(event) => onTypedChange(event.target.value)}
    />
  );
}

function useConfirmation(onConfirm: () => Promise<void>, onConfirmed: () => void) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent, confirmable: boolean) => {
    event.preventDefault();
    if (!confirmable || pending) return;
    setPending(true);
    setError(null);
    try {
      await onConfirm();
      onConfirmed();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  };

  return { pending, error, submit, clearError: () => setError(null) };
}
