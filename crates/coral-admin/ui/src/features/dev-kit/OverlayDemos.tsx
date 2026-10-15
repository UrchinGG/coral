import { useState } from "react";

import { Button } from "../../kit/Button";
import { ConfirmDialog, Dialog } from "../../kit/Dialog";
import { EntityLink } from "../../kit/EntityLink";
import { SecretRevealDialog } from "../../kit/SecretRevealDialog";
import { Section } from "../../kit/Section";
import { toast } from "../../kit/toast";
import { SAMPLE_KEY, SAMPLE_PLAYER } from "./sample-data";

type OpenDialog = "plain" | "confirm" | "failing" | null;

const DEMO_ACTION_MS = 800;
const CONFIRM_PHRASE = "Kestrel_";

export function OverlayDemos() {
  const [dialog, setDialog] = useState<OpenDialog>(null);
  const [secretOpen, setSecretOpen] = useState(false);
  const close = () => setDialog(null);
  return (
    <Section title="Dialogs and toasts">
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => setDialog("plain")}>Dialog</Button>
        <Button variant="danger" onClick={() => setDialog("confirm")}>
          Type-to-confirm
        </Button>
        <Button onClick={() => setDialog("failing")}>Confirm that fails</Button>
        <Button onClick={() => setSecretOpen(true)}>Reveal a secret</Button>
        <Button onClick={() => toast.ok("Flag dismissed for 24 h.", { action: { label: "Undo", run: () => toast.ok("Flag restored.") } })}>Toast with undo</Button>
        <Button onClick={() => toast.error("The server refused the change: tagging is disabled for this account.")}>Error toast</Button>
        <Button
          onClick={() => {
            const handle = toast.pending("Regenerating the key…");
            window.setTimeout(() => handle.settle("Key regenerated."), DEMO_ACTION_MS);
          }}
        >
          Pending toast
        </Button>
      </div>
      <Dialog
        open={dialog === "plain"}
        onClose={close}
        title="A plain dialog"
        actions={
          <>
            <Button onClick={() => toast.ok("Raised from inside the dialog, shown above it.")}>Toast</Button>
            <Button variant="primary" onClick={close}>
              Close
            </Button>
          </>
        }
      >
        <p className="m-0">Esc, the backdrop and the button all close it, and focus returns to the button that opened it.</p>
        <p className="m-0">
          Hover <EntityLink entity={{ kind: "player", uuid: SAMPLE_PLAYER }} />: its card, like any toast raised here, renders above the dialog.
        </p>
      </Dialog>
      <ConfirmDialog
        open={dialog === "confirm"}
        onClose={close}
        title={`Regenerate the API key of ${CONFIRM_PHRASE}?`}
        tone="danger"
        confirmLabel="Regenerate key"
        expect={CONFIRM_PHRASE}
        onConfirm={() => new Promise((resolve) => window.setTimeout(resolve, DEMO_ACTION_MS)).then(() => setSecretOpen(true))}
      >
        <p className="m-0">The current key stops working immediately and moves to key history.</p>
      </ConfirmDialog>
      <ConfirmDialog
        open={dialog === "failing"}
        onClose={close}
        title="Add a strike"
        confirmLabel="Add strike"
        onConfirm={() =>
          new Promise((_, reject) => window.setTimeout(() => reject(new Error("422 rejected: the member already has three strikes")), DEMO_ACTION_MS))
        }
      >
        <p className="m-0">The server error appears inline and the dialog stays open.</p>
      </ConfirmDialog>
      <SecretRevealDialog open={secretOpen} onClose={() => setSecretOpen(false)} title="New API key" description="Send this to the member. It is also shown in full on the account page." secret={SAMPLE_KEY} copyLabel="Copy new key" />
    </Section>
  );
}
