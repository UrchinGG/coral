import type { ReactNode } from "react";

import { Button } from "./Button";
import { CopyButton } from "./Copy";
import { Dialog } from "./Dialog";

type SecretRevealDialogProps = {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description: ReactNode;
  secret: string;
  copyLabel?: string;
};

export function SecretRevealDialog({ open, onClose, title, description, secret, copyLabel = "Copy" }: SecretRevealDialogProps) {
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      actions={
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      }
    >
      <p className="m-0">{description}</p>
      <div className="flex items-start gap-1">
        <span className="min-w-0 flex-1 rounded-md border border-border-strong bg-bg px-3 py-2.5 font-mono text-13 break-all text-entity-key">{secret}</span>
        <CopyButton text={secret} label={copyLabel} />
      </div>
    </Dialog>
  );
}
