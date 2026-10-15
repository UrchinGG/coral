import { useState } from "react";
import { useNavigate } from "react-router-dom";

import { keys } from "../../api/keys";
import { useAction, useOptimistic } from "../../api/mutate";
import { deletePlugin, setPluginFlag, type FlagChange } from "../../api/plugins";
import type { PluginDetail, PluginInfo } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { ConfirmDialog, Dialog } from "../../kit/Dialog";
import { Input } from "../../kit/Input";
import { Section } from "../../kit/Section";

const FLAG_DONE: Record<FlagChange["flag"], (on: boolean) => string> = {
  official: (on) => (on ? "Marked official" : "No longer official"),
  unlisted: (on) => (on ? "Unlisted" : "Listed again"),
  disabled: (on) => (on ? "Disabled" : "Enabled again"),
};

export function PluginActions({ slug, plugin }: { slug: string; plugin: PluginInfo }) {
  const navigate = useNavigate();
  const [disabling, setDisabling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const flag = useOptimistic<FlagChange, PluginDetail>({
    run: (change) => setPluginFlag(slug, change),
    target: keys.plugins.core(slug),
    apply: (current, change) => ({ ...current, plugin: withFlag(current.plugin, change) }),
    done: (change) => FLAG_DONE[change.flag](change.on),
    invalidates: [keys.plugins.all()],
  });
  const remove = useAction<void, void>({
    run: () => deletePlugin(slug),
    pending: () => `Deleting ${slug}…`,
    done: () => `${slug} deleted`,
    invalidates: [keys.plugins.all()],
    onDone: () => navigate("/plugins", { replace: true }),
  });

  return (
    <Section title="Actions">
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => flag.mutate({ flag: "official", on: !plugin.official })}>{plugin.official ? "Unset official" : "Mark official"}</Button>
        <Button onClick={() => flag.mutate({ flag: "unlisted", on: !plugin.unlisted })}>{plugin.unlisted ? "Relist" : "Unlist"}</Button>
        {plugin.disabled ? (
          <Button onClick={() => flag.mutate({ flag: "disabled", on: false })}>Re-enable</Button>
        ) : (
          <Button variant="danger" onClick={() => setDisabling(true)}>
            Disable…
          </Button>
        )}
        <Button variant="danger" onClick={() => setDeleting(true)}>
          Delete plugin…
        </Button>
      </div>
      <DisableDialog
        open={disabling}
        onClose={() => setDisabling(false)}
        onDisable={(reason) => {
          flag.mutate({ flag: "disabled", on: true, reason });
          setDisabling(false);
        }}
      />
      <ConfirmDialog
        open={deleting}
        onClose={() => setDeleting(false)}
        title={`Delete ${plugin.display_name}?`}
        confirmLabel="Delete plugin"
        tone="danger"
        expect={slug}
        onConfirm={() => remove.mutateAsync().then(() => undefined)}
      >
        <p className="m-0">This deletes the plugin together with every release, install record and review.</p>
      </ConfirmDialog>
    </Section>
  );
}

function DisableDialog({ open, onClose, onDisable }: { open: boolean; onClose: () => void; onDisable: (reason: string) => void }) {
  const [reason, setReason] = useState("");
  const close = () => {
    setReason("");
    onClose();
  };
  return (
    <Dialog
      open={open}
      onClose={close}
      title="Disable this plugin"
      actions={
        <>
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button
            variant="danger"
            disabled={reason.trim() === ""}
            onClick={() => {
              onDisable(reason.trim());
              setReason("");
            }}
          >
            Disable
          </Button>
        </>
      }
    >
      <p className="m-0">The reason is published with the plugin on Starfish's list of disabled plugins.</p>
      <Input label="Reason" value={reason} onChange={(event) => setReason(event.target.value)} autoFocus />
    </Dialog>
  );
}

function withFlag(plugin: PluginInfo, change: FlagChange): PluginInfo {
  switch (change.flag) {
    case "official":
      return { ...plugin, official: change.on };
    case "unlisted":
      return { ...plugin, unlisted: change.on };
    case "disabled":
      return { ...plugin, disabled: change.on, disabled_reason: change.on ? (change.reason ?? null) : null };
  }
}
