import { Lock, LockOpen, RefreshCw, ShieldAlert, UserCog } from "lucide-react";
import { useState } from "react";

import { accountActions, accountKeys } from "../../api/accounts";
import type { AccountCore, Budget, MemberView } from "../../api/types/accounts";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { ConfirmDialog } from "../../kit/Dialog";
import { Textarea } from "../../kit/Input";
import { SecretRevealDialog } from "../../kit/SecretRevealDialog";
import { Select } from "../../kit/Select";
import { plural } from "../../lib/format";
import { patchQuery, useAccountMutation, withMember } from "./useAccountMutation";
import { ACCESS_LEVELS, accessLabel, STRIKE_DURATIONS, STRIKE_REASON_MAX_CHARS } from "./vocabulary";

type ActionProps = { discordId: string; member: MemberView };
type DialogName = "regenerate" | "access" | "strike";

export function AccountActions({ core, member }: { core: AccountCore; member: MemberView }) {
  const discordId = core.account.discord_id;
  const [dialog, setDialog] = useState<DialogName | null>(null);
  const close = () => setDialog(null);
  return (
    <div className="flex flex-wrap items-start gap-1.5">
      <LockToggle discordId={discordId} member={member} />
      <Button icon={<RefreshCw className="size-3" />} onClick={() => setDialog("regenerate")}>
        Regenerate key
      </Button>
      <ResetLimits discordId={discordId} />
      <TaggingToggle discordId={discordId} member={member} />
      <Button icon={<UserCog className="size-3" />} onClick={() => setDialog("access")}>
        Access level
      </Button>
      <Button variant="danger" icon={<ShieldAlert className="size-3" />} onClick={() => setDialog("strike")}>
        Strike
      </Button>
      <RegenerateDialog open={dialog === "regenerate"} onClose={close} discordId={discordId} />
      <AccessDialog open={dialog === "access"} onClose={close} discordId={discordId} member={member} />
      <StrikeDialog open={dialog === "strike"} onClose={close} discordId={discordId} />
    </div>
  );
}

function LockToggle({ discordId, member }: ActionProps) {
  const toggle = useAccountMutation<boolean>(discordId, {
    run: (lock) => (lock ? accountActions.lock(discordId) : accountActions.unlock(discordId)),
    success: (_, lock) => (lock ? "Key locked; requests with it now get 403." : "Key unlocked."),
    optimistic: (core, lock) => withMember(core, () => ({ key_locked: lock })),
  });
  const lock = !member.key_locked;
  return (
    <ConfirmButton variant={lock ? "danger" : "default"} confirmLabel={lock ? "Lock key" : "Unlock key"} onConfirm={() => toggle.mutate(lock)} pending={toggle.isPending}>
      {lock ? <Lock className="size-3" /> : <LockOpen className="size-3" />}
      {lock ? "Lock key" : "Unlock key"}
    </ConfirmButton>
  );
}

function TaggingToggle({ discordId, member }: ActionProps) {
  const toggle = useAccountMutation<boolean>(discordId, {
    run: (disabled) => accountActions.setTagging(discordId, disabled),
    success: (_, disabled) => (disabled ? "Tagging disabled." : "Tagging enabled."),
    optimistic: (core, disabled) => withMember(core, () => ({ tagging_disabled: disabled })),
  });
  const disable = !member.tagging_disabled;
  return (
    <ConfirmButton confirmLabel={disable ? "Disable tagging" : "Enable tagging"} onConfirm={() => toggle.mutate(disable)} pending={toggle.isPending}>
      {disable ? "Disable tagging" : "Enable tagging"}
    </ConfirmButton>
  );
}

function ResetLimits({ discordId }: { discordId: string }) {
  const reset = useAccountMutation(discordId, {
    run: () => accountActions.resetLimits(discordId),
    success: (result) => `Cleared ${plural(result.cleared.length, "limiter")}: ${result.cleared.join(", ")}.`,
    patches: [patchQuery<Budget[], void>(accountKeys.section(discordId, "limits"), (budgets) => budgets.map((budget) => ({ ...budget, used: 0 })))],
  });
  return (
    <ConfirmButton confirmLabel="Clear every limiter" onConfirm={() => reset.mutate()} pending={reset.isPending}>
      Reset limits
    </ConfirmButton>
  );
}

function RegenerateDialog({ open, onClose, discordId }: { open: boolean; onClose: () => void; discordId: string }) {
  const [secret, setSecret] = useState<string | null>(null);
  const regenerate = useAccountMutation(discordId, {
    run: () => accountActions.regenerateKey(discordId),
    success: () => "New API key issued; the old key is retired.",
    inDialog: true,
  });
  return (
    <>
      <ConfirmDialog
        open={open}
        onClose={onClose}
        title="Regenerate API key"
        confirmLabel="Regenerate"
        tone="danger"
        expect="regenerate"
        onConfirm={async () => setSecret((await regenerate.mutateAsync()).api_key)}
      >
        <p className="m-0">The current key stops working at once and moves to key history. Requests that still send it are rejected.</p>
      </ConfirmDialog>
      <SecretRevealDialog
        open={secret !== null}
        onClose={() => setSecret(null)}
        title="New API key"
        description="The new key is active now. Copy it if you are handing it to the owner; the account rail shows it too."
        secret={secret ?? ""}
        copyLabel="Copy new API key"
      />
    </>
  );
}

function AccessDialog({ open, onClose, discordId, member }: { open: boolean; onClose: () => void } & ActionProps) {
  const [level, setLevel] = useState(String(member.access_level));
  const change = useAccountMutation<number>(discordId, {
    run: (next) => accountActions.setAccessLevel(discordId, next),
    success: (_, next) => `Access level set to ${accessLabel(next)}.`,
    inDialog: true,
  });
  return (
    <ConfirmDialog open={open} onClose={onClose} title="Change access level" confirmLabel="Set access level" onConfirm={() => change.mutateAsync(Number(level))}>
      <p className="m-0">Currently {accessLabel(member.access_level)}. Staff levels unlock review, tagging and dashboard commands in the bot.</p>
      <Select label="Access level" value={level} options={ACCESS_LEVELS.map((rank) => ({ value: String(rank.level), label: rank.label }))} onChange={setLevel} />
    </ConfirmDialog>
  );
}

function StrikeDialog({ open, onClose, discordId }: { open: boolean; onClose: () => void; discordId: string }) {
  const [reason, setReason] = useState("");
  const [duration, setDuration] = useState<(typeof STRIKE_DURATIONS)[number]["value"]>("30");
  const strike = useAccountMutation<{ reason: string; duration_days: number | null }>(discordId, {
    run: (input) => accountActions.addStrike(discordId, input),
    success: () => "Strike issued.",
    inDialog: true,
  });
  const issue = async () => {
    await strike.mutateAsync({ reason: reason.trim(), duration_days: duration === "none" ? null : Number(duration) });
    setReason("");
  };
  const tooLong = reason.length > STRIKE_REASON_MAX_CHARS;
  return (
    <ConfirmDialog open={open} onClose={onClose} title="Issue a strike" confirmLabel="Issue strike" tone="danger" onConfirm={issue}>
      <Textarea
        label="Reason"
        value={reason}
        onChange={(event) => setReason(event.target.value)}
        hint={`${reason.length} / ${STRIKE_REASON_MAX_CHARS} characters, shown to the user`}
        error={tooLong ? "Too long" : undefined}
      />
      <Select label="Lasts" value={duration} options={STRIKE_DURATIONS} onChange={setDuration} />
    </ConfirmDialog>
  );
}
