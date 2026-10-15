import { useState, type ReactNode } from "react";

import { keys } from "../../api/keys";
import { useAction } from "../../api/mutate";
import { SERVER_INVALIDATES, setConfigRole, setLinkChannel } from "../../api/server";
import type { Directory, ServerConfigValues } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { Section } from "../../kit/Section";
import { channelChoices, roleChoices, type PickerChoice } from "./choices";
import { DiscordPicker } from "./DiscordPicker";
import { membersPhrase, startedText } from "./labels";

type LinkSettingsProps = { config: ServerConfigValues; directory: Directory | undefined; memberCount: number | undefined };

type ConfigRowProps = {
  label: string;
  description: ReactNode;
  saved: string | null;
  choices: readonly PickerChoice[] | undefined;
  placeholder: string;
  prefix: "@" | "#";
  confirmLabel: string;
  pending: boolean;
  onApply: (id: string | null) => void;
};

export function LinkSettings({ config, directory, memberCount }: LinkSettingsProps) {
  const roles = roleChoices(directory?.roles);
  const swap = `Update ${membersPhrase(memberCount)}`;
  const linkRole = useRoleAction("link-role", "Linked role");
  const unlinkedRole = useRoleAction("unlinked-role", "Unlinked role");
  const channel = useAction({
    run: setLinkChannel,
    pending: () => "Moving the link prompt…",
    done: () => "Link channel updated",
    invalidates: [keys.server.config()],
  });

  return (
    <Section title="Account linking">
      <div className="flex flex-col divide-y divide-border">
        <ConfigRow
          key={`link:${config.link_role_id ?? ""}`}
          label="Linked role"
          description="Assigned to members once their Minecraft account is linked. Changing it swaps the role across the whole server."
          saved={config.link_role_id}
          choices={roles}
          placeholder="Select a linked role"
          prefix="@"
          confirmLabel={swap}
          pending={linkRole.isPending}
          onApply={(id) => linkRole.mutate(id)}
        />
        <ConfigRow
          key={`unlinked:${config.unlinked_role_id ?? ""}`}
          label="Unlinked role"
          description="Held by members who haven't linked an account yet. Changing it swaps the role across the whole server."
          saved={config.unlinked_role_id}
          choices={roles}
          placeholder="Select an unlinked role"
          prefix="@"
          confirmLabel={swap}
          pending={unlinkedRole.isPending}
          onApply={(id) => unlinkedRole.mutate(id)}
        />
        <ConfigRow
          key={`channel:${config.link_channel_id ?? ""}`}
          label="Link channel"
          description="Coral keeps a persistent “Link your Minecraft account” prompt here. Changing it deletes the old prompt and posts a new one."
          saved={config.link_channel_id}
          choices={channelChoices(directory?.channels)}
          placeholder="Select a link channel"
          prefix="#"
          confirmLabel="Repost link prompt"
          pending={channel.isPending}
          onApply={(id) => channel.mutate(id)}
        />
      </div>
    </Section>
  );
}

function useRoleAction(field: "link-role" | "unlinked-role", noun: string) {
  return useAction({
    run: (roleId: string | null) => setConfigRole(field, roleId),
    pending: () => `Saving the ${noun.toLowerCase()}…`,
    done: (started) => startedText(started, `${noun} updated`),
    invalidates: SERVER_INVALIDATES,
  });
}

function ConfigRow({ label, description, saved, choices, placeholder, prefix, confirmLabel, pending, onApply }: ConfigRowProps) {
  const [draft, setDraft] = useState<string | null>(saved);
  const dirty = draft !== saved;
  return (
    <div className="flex flex-wrap items-start gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-[1_1_16rem]">
        <div className="text-13 font-medium">{label}</div>
        <div className="mt-0.5 text-12 text-muted">{description}</div>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <DiscordPicker label={label} choices={choices} value={draft} onChange={setDraft} placeholder={placeholder} prefix={prefix} />
        {dirty && (
          <>
            <Button variant="primary" pending={pending} onClick={() => onApply(draft)}>
              {confirmLabel}
            </Button>
            <Button variant="ghost" onClick={() => setDraft(saved)}>
              Reset
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
