import { useState } from "react";

import { accountActions } from "../../api/accounts";
import type { DevKey } from "../../api/types/accounts";
import { Pill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { CopyField } from "../../kit/Copy";
import { ConfirmDialog } from "../../kit/Dialog";
import { Input } from "../../kit/Input";
import { KeyValue } from "../../kit/KeyValue";
import { SecretRevealDialog } from "../../kit/SecretRevealDialog";
import { Section } from "../../kit/Section";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { useAccountMutation } from "./useAccountMutation";
import { DEV_KEY_DEFAULT_RATE_LIMIT, DEV_KEY_LABEL_MAX_CHARS, DEV_KEY_MAX_RATE_LIMIT, DEV_KEY_PERMISSIONS, permissionLabels } from "./vocabulary";

type DevKeyDialog = "create" | "rate-limit" | "permissions";
type DialogProps = { open: boolean; onClose: () => void; discordId: string };

export function DeveloperKey({ discordId, devKey }: { discordId: string; devKey: DevKey | null }) {
  const [dialog, setDialog] = useState<DevKeyDialog | null>(null);
  const close = () => setDialog(null);
  return (
    <Section title="Developer key" actions={devKey === null ? <Button size="sm" onClick={() => setDialog("create")}>Create</Button> : undefined}>
      {devKey === null ? <span className="text-12 text-subtle">None</span> : <DevKeyDetails discordId={discordId} devKey={devKey} onEdit={setDialog} />}
      <CreateDialog open={dialog === "create"} onClose={close} discordId={discordId} />
      {devKey !== null && (
        <>
          <RateLimitDialog open={dialog === "rate-limit"} onClose={close} discordId={discordId} current={devKey.rate_limit} />
          <PermissionsDialog open={dialog === "permissions"} onClose={close} discordId={discordId} current={devKey.permissions} />
        </>
      )}
    </Section>
  );
}

function DevKeyDetails({ discordId, devKey, onEdit }: { discordId: string; devKey: DevKey; onEdit: (dialog: DevKeyDialog) => void }) {
  const { time } = useTimeFormat();
  const lock = useAccountMutation<boolean>(discordId, {
    run: (locked) => accountActions.setDevKeyLocked(discordId, locked),
    success: (_, locked) => (locked ? "Developer key locked." : "Developer key unlocked."),
    optimistic: (core, locked) => (core.keys.developer === null ? core : { ...core, keys: { ...core.keys, developer: { ...core.keys.developer, locked } } }),
  });
  const remove = useAccountMutation(discordId, { run: () => accountActions.deleteDevKey(discordId), success: () => "Developer key deleted." });
  return (
    <div className="flex flex-col gap-2.5">
      <CopyField value={devKey.key} label="Copy developer key" secret />
      <KeyValue
        items={[
          { label: "Label", value: devKey.label },
          { label: "State", value: devKey.locked ? <Pill tone="danger">locked</Pill> : <Pill tone="ok">active</Pill> },
          { label: "Permissions", value: permissionLabels(devKey.permissions) },
          { label: "Rate limit", value: `${formatNumber(devKey.rate_limit)} per 5 min` },
          { label: "Requests", value: formatNumber(devKey.request_count) },
          { label: "Created", value: time(devKey.created_at, { seconds: false, date: "always" }) },
        ]}
      />
      <div className="flex flex-wrap gap-1.5">
        <ConfirmButton size="sm" confirmLabel={devKey.locked ? "Unlock" : "Lock"} onConfirm={() => lock.mutate(!devKey.locked)} pending={lock.isPending}>
          {devKey.locked ? "Unlock" : "Lock"}
        </ConfirmButton>
        <Button size="sm" onClick={() => onEdit("rate-limit")}>
          Rate limit
        </Button>
        <Button size="sm" onClick={() => onEdit("permissions")}>
          Permissions
        </Button>
        <ConfirmButton size="sm" variant="danger" confirmLabel="Delete key" onConfirm={() => remove.mutate()} pending={remove.isPending}>
          Delete
        </ConfirmButton>
      </div>
    </div>
  );
}

function CreateDialog({ open, onClose, discordId }: DialogProps) {
  const [label, setLabel] = useState("");
  const [rateLimit, setRateLimit] = useState(String(DEV_KEY_DEFAULT_RATE_LIMIT));
  const [permissions, setPermissions] = useState(0);
  const [created, setCreated] = useState<string | null>(null);
  const create = useAccountMutation<void, DevKey>(discordId, {
    run: () => accountActions.createDevKey(discordId, { label: label.trim(), permissions, rate_limit: Number(rateLimit) }),
    success: () => "Developer key created.",
    inDialog: true,
  });
  return (
    <>
      <ConfirmDialog open={open} onClose={onClose} title="Create a developer key" confirmLabel="Create key" onConfirm={async () => setCreated((await create.mutateAsync()).key)}>
        <Input label="Label" value={label} maxLength={DEV_KEY_LABEL_MAX_CHARS} onChange={(event) => setLabel(event.target.value)} />
        <RateLimitInput value={rateLimit} onChange={setRateLimit} />
        <PermissionChecks value={permissions} onChange={setPermissions} />
      </ConfirmDialog>
      <SecretRevealDialog
        open={created !== null}
        onClose={() => setCreated(null)}
        title="New developer key"
        description="The key is active now. Copy it if you are handing it to the developer; the account rail shows it too."
        secret={created ?? ""}
        copyLabel="Copy developer key"
      />
    </>
  );
}

function RateLimitDialog({ open, onClose, discordId, current }: DialogProps & { current: number }) {
  const [rateLimit, setRateLimit] = useState(String(current));
  const change = useAccountMutation<number>(discordId, {
    run: (limit) => accountActions.setDevKeyRateLimit(discordId, limit),
    success: (_, limit) => `Developer key limited to ${formatNumber(limit)} requests per 5 min.`,
    inDialog: true,
  });
  return (
    <ConfirmDialog open={open} onClose={onClose} title="Developer key rate limit" confirmLabel="Save" onConfirm={() => change.mutateAsync(Number(rateLimit))}>
      <RateLimitInput value={rateLimit} onChange={setRateLimit} />
    </ConfirmDialog>
  );
}

function PermissionsDialog({ open, onClose, discordId, current }: DialogProps & { current: number }) {
  const [permissions, setPermissions] = useState(current);
  const change = useAccountMutation<number>(discordId, {
    run: (bits) => accountActions.setDevKeyPermissions(discordId, bits),
    success: (_, bits) => `Developer key permissions: ${permissionLabels(bits)}.`,
    inDialog: true,
  });
  return (
    <ConfirmDialog open={open} onClose={onClose} title="Developer key permissions" confirmLabel="Save" onConfirm={() => change.mutateAsync(permissions)}>
      <PermissionChecks value={permissions} onChange={setPermissions} />
    </ConfirmDialog>
  );
}

function RateLimitInput({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return (
    <Input label="Requests per 5 minutes" type="number" min={1} max={DEV_KEY_MAX_RATE_LIMIT} value={value} onChange={(event) => onChange(event.target.value)} mono />
  );
}

function PermissionChecks({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return (
    <fieldset className="m-0 flex flex-col gap-1.5 border-0 p-0">
      <legend className="mb-1 text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Permissions</legend>
      {DEV_KEY_PERMISSIONS.map((permission) => (
        <label key={permission.bit} className="flex items-center gap-2 text-12 text-fg">
          <input type="checkbox" checked={(value & permission.bit) !== 0} onChange={(event) => onChange(event.target.checked ? value | permission.bit : value & ~permission.bit)} />
          {permission.label}
        </label>
      ))}
    </fieldset>
  );
}
