import { useState } from "react";

import type { ViewAction } from "../../api/types/extension";
import { Button } from "../../kit/Button";
import { ConfirmDialog } from "../../kit/Dialog";
import { initialValues, missingRequired } from "./field-values";
import type { ActionRun } from "./useExtensionAction";
import { ViewFields } from "./ViewFields";

type ViewActionButtonProps = { action: ViewAction; pending: boolean; onRun: (run: ActionRun) => Promise<unknown>; size?: "sm" | "md" };

export function ViewActionButton({ action, pending, onRun, size = "sm" }: ViewActionButtonProps) {
  const [open, setOpen] = useState(false);
  const direct = action.confirm === null && action.fields.length === 0;
  const variant = action.danger ? "danger" : "default";
  if (direct) {
    return (
      <Button size={size} variant={variant} pending={pending} onClick={() => void onRun({ action, values: {} }).catch(() => undefined)}>
        {action.label}
      </Button>
    );
  }
  return (
    <>
      <Button size={size} variant={variant} pending={pending} onClick={() => setOpen(true)}>
        {action.label}
      </Button>
      {open && <ActionDialog action={action} onClose={() => setOpen(false)} onRun={onRun} />}
    </>
  );
}

function ActionDialog({ action, onClose, onRun }: { action: ViewAction; onClose: () => void; onRun: (run: ActionRun) => Promise<unknown> }) {
  const [values, setValues] = useState(() => initialValues(action.fields));
  const confirm = action.confirm;
  return (
    <ConfirmDialog
      open
      onClose={onClose}
      title={confirm?.title ?? action.label}
      confirmLabel={action.label}
      tone={action.danger ? "danger" : "primary"}
      expect={confirm?.phrase ?? undefined}
      onConfirm={async () => {
        if (missingRequired(action.fields, values)) throw new Error(requiredMessage(action));
        await onRun({ action, values });
      }}
    >
      {confirm !== null && <p className="m-0">{confirm.text}</p>}
      {action.fields.length > 0 && <ViewFields fields={action.fields} values={values} onChange={setValues} />}
    </ConfirmDialog>
  );
}

function requiredMessage(action: ViewAction): string {
  const labels = action.fields.filter((field) => field.required).map((field) => field.label);
  return `Required: ${labels.join(", ")}`;
}
