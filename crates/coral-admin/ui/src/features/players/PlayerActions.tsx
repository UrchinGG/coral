import { useQuery } from "@tanstack/react-query";
import { Ban, Lock, LockOpen, Tag } from "lucide-react";
import { useState, type ReactNode } from "react";

import { playerTagsQuery, usePlayerMutations } from "../../api/players";
import type { LockView, TagType } from "../../api/types/players";
import { Button } from "../../kit/Button";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { ConfirmDialog } from "../../kit/Dialog";
import { Input, Textarea } from "../../kit/Input";
import { Select } from "../../kit/Select";
import { toast } from "../../kit/toast";

type PlayerActionsProps = { uuid: string; locked: LockView | null; blocked: LockView | null };

type Dialog = "tag" | "lock" | "unlock" | "block" | null;

export function PlayerActions({ uuid, locked, blocked }: PlayerActionsProps) {
  const [dialog, setDialog] = useState<Dialog>(null);
  const { unblock } = usePlayerMutations(uuid);
  const close = () => setDialog(null);
  const confirmUnblock = () =>
    unblock.mutate(undefined, {
      onSuccess: () => toast.ok("This Minecraft account can sign in again."),
      onError: (error) => toast.error(`Could not unblock sign-in: ${error.message}`),
    });
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button icon={<Tag className="size-3" />} onClick={() => setDialog("tag")}>
        Add tag…
      </Button>
      {locked === null ? (
        <Button icon={<Lock className="size-3" />} onClick={() => setDialog("lock")}>
          Lock
        </Button>
      ) : (
        <Button icon={<LockOpen className="size-3" />} onClick={() => setDialog("unlock")}>
          Unlock
        </Button>
      )}
      {blocked === null ? (
        <Button icon={<Ban className="size-3" />} onClick={() => setDialog("block")}>
          Block sign-in
        </Button>
      ) : (
        <ConfirmButton confirmLabel="Unblock" pending={unblock.isPending} onConfirm={confirmUnblock}>
          Unblock sign-in
        </ConfirmButton>
      )}
      {dialog === "tag" && <AddTagDialog uuid={uuid} onClose={close} />}
      {dialog === "lock" && <LockDialog uuid={uuid} onClose={close} />}
      {dialog === "unlock" && <UnlockDialog uuid={uuid} onClose={close} />}
      {dialog === "block" && <BlockDialog uuid={uuid} onClose={close} />}
    </div>
  );
}

function AddTagDialog({ uuid, onClose }: { uuid: string; onClose: () => void }) {
  const tags = useQuery(playerTagsQuery(uuid));
  const types = tags.data?.tag_types ?? [];
  const [chosen, setChosen] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  const [hideUsername, setHideUsername] = useState(false);
  const { addTag } = usePlayerMutations(uuid);
  const tagType = chosen ?? types[0]?.name ?? null;

  const confirm = async () => {
    if (tagType === null) throw new Error("The tag types have not loaded yet.");
    if (reason.trim() === "") throw new Error("A tag needs a reason.");
    await addTag.mutateAsync({ tag_type: tagType, reason: reason.trim(), hide_username: hideUsername });
    toast.ok(`Tagged ${tagLabel(types, tagType)}.`);
  };

  return (
    <ConfirmDialog open onClose={onClose} title="Add tag" confirmLabel="Add tag" onConfirm={confirm}>
      {tags.error !== null && <span className="text-danger">Tag types failed to load: {tags.error.message}</span>}
      {tagType !== null && (
        <Select
          label="Tag"
          value={tagType}
          options={types.map((type) => ({ value: type.name, label: type.display_name }))}
          onChange={setChosen}
        />
      )}
      <Textarea label="Reason" value={reason} onChange={(event) => setReason(event.target.value)} rows={3} />
      <label className="flex items-center gap-2 text-12 text-muted">
        <input type="checkbox" checked={hideUsername} onChange={(event) => setHideUsername(event.target.checked)} className="size-3.5 accent-accent" />
        Hide the username on overlays
      </label>
    </ConfirmDialog>
  );
}

function LockDialog({ uuid, onClose }: { uuid: string; onClose: () => void }) {
  const { lock } = usePlayerMutations(uuid);
  const confirm = async (reason: string | null) => {
    await lock.mutateAsync(reason);
    toast.ok("Player locked; only staff can change its tags now.");
  };
  return (
    <ReasonDialog title="Lock player" confirmLabel="Lock" onConfirm={confirm} onClose={onClose}>
      Locked players keep their tags until staff change them.
    </ReasonDialog>
  );
}

function BlockDialog({ uuid, onClose }: { uuid: string; onClose: () => void }) {
  const { block } = usePlayerMutations(uuid);
  const confirm = async (reason: string | null) => {
    await block.mutateAsync(reason);
    toast.ok("This Minecraft account can no longer sign in.");
  };
  return (
    <ReasonDialog title="Block Minecraft sign-in" confirmLabel="Block" onConfirm={confirm} onClose={onClose}>
      This account can no longer sign in to Urchin from the mod, and the sessions it already has stop working.
    </ReasonDialog>
  );
}

type ReasonDialogProps = { title: string; confirmLabel: string; children: ReactNode; onConfirm: (reason: string | null) => Promise<void>; onClose: () => void };

function ReasonDialog({ title, confirmLabel, children, onConfirm, onClose }: ReasonDialogProps) {
  const [reason, setReason] = useState("");
  return (
    <ConfirmDialog open onClose={onClose} title={title} confirmLabel={confirmLabel} tone="danger" onConfirm={() => onConfirm(reason.trim() === "" ? null : reason.trim())}>
      <span>{children}</span>
      <Input label="Reason (optional)" value={reason} onChange={(event) => setReason(event.target.value)} />
    </ConfirmDialog>
  );
}

function UnlockDialog({ uuid, onClose }: { uuid: string; onClose: () => void }) {
  const { unlock } = usePlayerMutations(uuid);
  const confirm = async () => {
    await unlock.mutateAsync();
    toast.ok("Player unlocked.");
  };
  return (
    <ConfirmDialog open onClose={onClose} title="Unlock player" confirmLabel="Unlock" onConfirm={confirm}>
      <span>Anyone allowed to tag can change this player's tags again.</span>
    </ConfirmDialog>
  );
}

function tagLabel(types: readonly TagType[], name: string): string {
  return types.find((type) => type.name === name)?.display_name ?? name;
}
