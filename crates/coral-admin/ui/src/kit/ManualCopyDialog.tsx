import { useEffect, useRef } from "react";

import { Button } from "./Button";
import { Dialog } from "./Dialog";

export function ManualCopyDialog({ text, onClose }: { text: string; onClose: () => void }) {
  const areaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    areaRef.current?.focus();
    areaRef.current?.select();
  }, []);

  return (
    <Dialog
      open
      onClose={onClose}
      title="Copy manually"
      actions={
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      }
    >
      <p className="m-0">Clipboard access is blocked in this view. The text below is selected; press Ctrl+C or ⌘C.</p>
      <textarea ref={areaRef} readOnly rows={8} value={text} className="rounded-md border border-border-strong bg-bg p-2 font-mono text-12 text-fg" />
    </Dialog>
  );
}
