import { type ReactNode, useState } from "react";
import { useDiscordServer, useSetLinkChannel, useSetLinkRole, useSetUnlinkedRole } from "../api/discord";
import type { DiscordServerDetail } from "../api/types";
import { AutorolesPanel } from "../components/AutorolesPanel";
import { NicknameTemplatePanel } from "../components/NicknameTemplatePanel";
import { Panel } from "../components/Panel";
import { PickerSelect, type PickerItem } from "../components/PickerSelect";
import { SyncJobsPanel } from "../components/SyncJobsPanel";
import { fmtNum } from "../format";

export function ServerSync() {
  const server = useDiscordServer();

  if (server.isLoading) {
    return <div className="text-sm text-gray-500">Loading…</div>;
  }
  if (!server.data) {
    return <div className="text-sm text-gray-500">Home guild not configured.</div>;
  }

  const detail = server.data;
  const { guild, config } = detail;

  return (
    <div className="flex flex-col gap-6">
      <Panel>
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-lg font-semibold text-gray-100">{guild.name}</h1>
          <span className="font-mono text-xs text-gray-600">{guild.guild_id}</span>
        </div>
        <div className="mt-1 text-xs text-gray-500">
          {fmtNum(guild.member_count)} members · {fmtNum(guild.linked_members)} linked
        </div>
      </Panel>

      <SyncJobsPanel />

      <LinkSettingsPanel detail={detail} />

      <NicknameTemplatePanel
        key={config.nickname_template ?? "unset"}
        template={config.nickname_template}
        previewContext={detail.preview_context}
        memberCount={guild.member_count}
      />

      <AutorolesPanel
        rules={detail.rules}
        roles={detail.roles}
        previewContext={detail.preview_context}
        memberCount={guild.member_count}
      />
    </div>
  );
}

function LinkSettingsPanel({ detail }: { detail: DiscordServerDetail }) {
  const { guild, config } = detail;
  const setLinkRole = useSetLinkRole();
  const setUnlinkedRole = useSetUnlinkedRole();
  const setLinkChannel = useSetLinkChannel();

  const roleItems: PickerItem[] = detail.roles.map((role) => ({
    id: role.id,
    label: role.name,
    color: role.color,
    disabled: !role.assignable,
    hint: role.managed ? "managed" : !role.assignable ? "not manageable" : undefined,
  }));

  const channelItems: PickerItem[] = detail.channels.map((channel) => ({
    id: channel.id,
    label: channel.name,
    hint: channel.category ?? undefined,
  }));

  const swapConfirm = `Update ${fmtNum(guild.member_count)} members`;

  return (
    <Panel title="Account linking" description="Roles and the link prompt Coral manages for this server.">
      <div className="flex flex-col divide-y divide-white/5">
        <ConfigRow
          key={`link:${config.link_role_id ?? "none"}`}
          label="Linked role"
          description="Assigned to members once their Minecraft account is linked. Changing it swaps the role across the whole server."
          saved={config.link_role_id}
          items={roleItems}
          placeholder="Select a linked role"
          prefix="@"
          confirmLabel={swapConfirm}
          onApply={(id) => setLinkRole.mutate(id)}
          pending={setLinkRole.isPending}
        />
        <ConfigRow
          key={`unlinked:${config.unlinked_role_id ?? "none"}`}
          label="Unlinked role"
          description="Held by members who haven't linked an account yet. Changing it swaps the role across the whole server."
          saved={config.unlinked_role_id}
          items={roleItems}
          placeholder="Select an unlinked role"
          prefix="@"
          confirmLabel={swapConfirm}
          onApply={(id) => setUnlinkedRole.mutate(id)}
          pending={setUnlinkedRole.isPending}
        />
        <ConfigRow
          key={`channel:${config.link_channel_id ?? "none"}`}
          label="Link channel"
          description="Coral keeps a persistent “Link your Minecraft account” prompt here. Changing it deletes the old prompt and posts a new one."
          saved={config.link_channel_id}
          items={channelItems}
          placeholder="Select a link channel"
          prefix="#"
          confirmLabel="Repost link prompt"
          onApply={(id) => setLinkChannel.mutate(id)}
          pending={setLinkChannel.isPending}
        />
      </div>
    </Panel>
  );
}

function ConfigRow({
  label,
  description,
  saved,
  items,
  placeholder,
  prefix,
  confirmLabel,
  onApply,
  pending,
}: {
  label: string;
  description: ReactNode;
  saved: string | null;
  items: PickerItem[];
  placeholder: string;
  prefix: string;
  confirmLabel: string;
  onApply: (id: string | null) => void;
  pending: boolean;
}) {
  const [draft, setDraft] = useState<string | null>(saved);
  const dirty = draft !== saved;

  return (
    <div className="flex flex-wrap items-start gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-1 basis-64">
        <div className="text-sm font-medium text-gray-200">{label}</div>
        <div className="mt-0.5 text-xs text-gray-500">{description}</div>
      </div>
      <div className="flex items-center gap-2">
        <PickerSelect items={items} value={draft} onChange={setDraft} placeholder={placeholder} prefix={prefix} />
        {dirty && (
          <>
            <button
              disabled={pending}
              onClick={() => onApply(draft)}
              className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25 disabled:opacity-50"
            >
              {pending ? "Applying…" : confirmLabel}
            </button>
            <button onClick={() => setDraft(saved)} className="text-xs text-gray-500 hover:text-gray-300">
              Reset
            </button>
          </>
        )}
      </div>
    </div>
  );
}
