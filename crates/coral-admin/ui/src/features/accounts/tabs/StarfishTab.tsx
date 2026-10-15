import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { accountActions, accountKeys, LICENSE_STATUSES, starfishQuery, type LicenseStatus } from "../../../api/accounts";
import type { AccountCore, StarfishAccount, StarfishHwid } from "../../../api/types/accounts";
import { Badge, Pill } from "../../../kit/Badges";
import { ConfirmButton } from "../../../kit/ConfirmButton";
import { EntityLink } from "../../../kit/EntityLink";
import { KeyValue } from "../../../kit/KeyValue";
import { QueryContent } from "../../../kit/QueryContent";
import { Section } from "../../../kit/Section";
import { Select } from "../../../kit/Select";
import { Table, type Column } from "../../../kit/Table";
import { useTimeFormat } from "../../../lib/time";
import { patchQuery, useAccountMutation } from "../useAccountMutation";
import { LICENSE_TONE } from "../vocabulary";

type Session = StarfishAccount["sessions"][number];
type Installed = StarfishAccount["installed_plugins"][number];

const COMPONENTS: readonly { key: keyof StarfishHwid; label: string }[] = [
  { key: "machine_guid_hash", label: "machine GUID" },
  { key: "smbios_uuid_hash", label: "SMBIOS UUID" },
  { key: "disk_serial_hash", label: "disk serial" },
  { key: "cpu_id_hash", label: "CPU id" },
  { key: "baseboard_serial_hash", label: "baseboard serial" },
];

export function StarfishTab({ core }: { core: AccountCore }) {
  const discordId = core.account.discord_id;
  const starfish = useQuery(starfishQuery(discordId));
  return (
    <QueryContent query={starfish} isEmpty={(data) => data === null} empty="No Starfish account for this Discord user.">
      {(data) => data !== null && <StarfishDetail discordId={discordId} account={data} fetching={starfish.isFetching} />}
    </QueryContent>
  );
}

function StarfishDetail({ discordId, account, fetching }: { discordId: string; account: StarfishAccount; fetching: boolean }) {
  const { time } = useTimeFormat();
  const { user } = account;
  return (
    <>
      <Section title="License" fetching={fetching} actions={<SessionRevoke discordId={discordId} active={account.sessions.some((session) => session.active)} />}>
        <KeyValue
          items={[
            { label: "Starfish user", value: <span className="font-mono">#{user.id}</span> },
            { label: "License", value: <LicenseControl discordId={discordId} status={user.license_status} /> },
            { label: "GitHub", value: user.github_username === null ? <span className="text-subtle">not linked</span> : <span className="font-mono">{user.github_username}</span> },
            { label: "Linked GitHub", value: user.github_linked_at === null ? "—" : time(user.github_linked_at, { seconds: false, date: "always" }) },
            { label: "Created", value: time(user.created_at, { seconds: false, date: "always" }) },
          ]}
        />
      </Section>
      <Section title="HWIDs" count={account.hwids.length} flush>
        <HwidTable hwids={account.hwids} />
      </Section>
      <Section title="Sessions" count={account.sessions.length} flush>
        <SessionTable sessions={account.sessions} />
      </Section>
      <Section title="Plugins" count={account.installed_plugins.length + account.owned_plugins.length} flush>
        <PluginTable account={account} />
      </Section>
    </>
  );
}

function LicenseControl({ discordId, status }: { discordId: string; status: string }) {
  const change = useAccountMutation<LicenseStatus>(discordId, {
    run: (next) => accountActions.setLicense(discordId, next),
    success: (_, next) => `Starfish license set to ${next}.`,
    optimistic: (core, next) => (core.starfish === null ? core : { ...core, starfish: { ...core.starfish, license_status: next } }),
    patches: [starfishPatch<LicenseStatus>(discordId, (account, next) => ({ ...account, user: { ...account.user, license_status: next } }))],
  });
  const known = LICENSE_STATUSES.find((candidate) => candidate === status);
  return (
    <span className="inline-flex flex-wrap items-center gap-2">
      <Pill tone={LICENSE_TONE[status] ?? "neutral"}>{status}</Pill>
      <Select aria-label="Change license" value={known ?? LICENSE_STATUSES[0]} options={LICENSE_STATUSES.map((value) => ({ value, label: value }))} onChange={(next) => next !== status && change.mutate(next)} />
    </span>
  );
}

function SessionRevoke({ discordId, active }: { discordId: string; active: boolean }) {
  const revoke = useAccountMutation(discordId, {
    run: () => accountActions.revokeSessions(discordId),
    success: () => "Starfish sessions and refresh tokens revoked.",
    optimistic: (core) => (core.starfish === null ? core : { ...core, starfish: { ...core.starfish, active_session: false } }),
    patches: [starfishPatch<void>(discordId, (account) => ({ ...account, sessions: [], refresh_tokens: [] }))],
  });
  return (
    <ConfirmButton size="sm" variant="danger" confirmLabel="Revoke every session" onConfirm={() => revoke.mutate()} pending={revoke.isPending} disabled={!active}>
      Revoke sessions
    </ConfirmButton>
  );
}

function starfishPatch<V>(discordId: string, change: (account: StarfishAccount, variables: V) => StarfishAccount) {
  return patchQuery<StarfishAccount | null, V>(accountKeys.section(discordId, "starfish"), (current, variables) => (current === null ? null : change(current, variables)));
}

function HwidTable({ hwids }: { hwids: readonly StarfishHwid[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<StarfishHwid>[]>(
    () => [
      { id: "hash", header: "HWID", cell: (row) => <span className="font-mono text-[11.5px] break-all">{row.hwid_hash}</span> },
      { id: "state", header: "State", cell: (row) => (row.is_active ? <Pill tone="ok">active</Pill> : <Pill>inactive</Pill>) },
      { id: "registered", header: "Registered", cell: (row) => time(row.registered_at, { seconds: false, date: "always" }), priority: 2 },
      { id: "components", header: "Components", cell: (row) => <Components hwid={row} /> },
    ],
    [time],
  );
  return <Table label="Registered hardware ids" columns={columns} rows={hwids} rowKey={(row) => String(row.id)} empty="No hardware registered." />;
}

function Components({ hwid }: { hwid: StarfishHwid }) {
  if (hwid.components_at === null) return <span className="text-subtle">not reported</span>;
  return (
    <span className="flex flex-col gap-0.5">
      {COMPONENTS.map(({ key, label }) => (
        <span key={key} className="text-11">
          <span className="text-subtle">{label}</span> <span className="font-mono break-all">{String(hwid[key] ?? "—")}</span>
        </span>
      ))}
    </span>
  );
}

function SessionTable({ sessions }: { sessions: readonly Session[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<Session>[]>(
    () => [
      { id: "session", header: "Session", cell: (row) => <span className="font-mono">#{row.id}</span> },
      { id: "hwid", header: "HWID", cell: (row) => <span className="font-mono">#{row.hwid_id}</span>, priority: 2 },
      { id: "state", header: "State", cell: (row) => (row.active ? <Pill tone="ok">open</Pill> : <Pill>expired</Pill>) },
      { id: "issued", header: "Issued", cell: (row) => time(row.issued_at, { seconds: false, date: "always" }) },
      { id: "heartbeat", header: "Last heartbeat", cell: (row) => time(row.last_heartbeat_at, { seconds: false, date: "always" }) },
      { id: "expires", header: "Expires", cell: (row) => time(row.expires_at, { seconds: false, date: "always" }), priority: 2 },
    ],
    [time],
  );
  return <Table label="Starfish sessions" columns={columns} rows={sessions} rowKey={(row) => String(row.id)} empty="No sessions." />;
}

function PluginTable({ account }: { account: StarfishAccount }) {
  const rows: (Installed & { owned: boolean })[] = [
    ...account.owned_plugins.map((plugin) => ({ plugin: plugin.plugin, installed_version: "—", latest_version: plugin.latest_version ?? "—", disabled: plugin.disabled, owned: true })),
    ...account.installed_plugins.map((plugin) => ({ ...plugin, owned: false })),
  ];
  return (
    <Table
      label="Plugins this user owns or installed"
      columns={PLUGIN_COLUMNS}
      rows={rows}
      rowKey={(row) => `${row.owned ? "owned" : "installed"}:${row.plugin.slug}`}
      empty="No plugins owned or installed."
    />
  );
}

const PLUGIN_COLUMNS: readonly Column<Installed & { owned: boolean }>[] = [
  { id: "plugin", header: "Plugin", cell: (row) => <EntityLink entity={row.plugin} /> },
  { id: "relation", header: "Relation", cell: (row) => <Badge>{row.owned ? "owner" : "installed"}</Badge> },
  { id: "installed", header: "Installed", cell: (row) => <span className="font-mono">{row.installed_version}</span> },
  { id: "latest", header: "Latest", cell: (row) => <span className="font-mono">{row.latest_version}</span> },
  { id: "state", header: "State", cell: (row) => (row.disabled ? <Pill tone="warning">disabled</Pill> : <Pill tone="ok">enabled</Pill>) },
];
