import { useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import {
  useGuildAt,
  useGuildDetail,
  useGuildSnapshots,
  usePlayerSnapshotAt,
  usePlayerSnapshotDetail,
  usePlayerSnapshots,
  useResolveIds,
} from "../api/data";
import type { GuildRow, PlayerSnapshotRow } from "../api/types";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { fmtDate, fmtNum, prettyJson } from "../format";

const PAGE_SIZE = 50;
const TABS = ["players", "guilds", "resolve"] as const;
type Tab = (typeof TABS)[number];

const TAB_LABELS: Record<Tab, string> = {
  players: "Player snapshots",
  guilds: "Guild snapshots",
  resolve: "ID resolver",
};

export function Data() {
  const [tab, setTab] = useState<Tab>("players");

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold text-gray-100">Data</h1>
          <p className="mt-1 text-xs text-gray-500">Raw Hypixel snapshot cache — low-level debugging, not for routine moderation.</p>
        </div>
        <div className="flex overflow-hidden rounded-md border border-white/8">
          {TABS.map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={`px-3 py-1.5 text-xs font-medium ${t === tab ? "bg-accent/15 text-accent" : "text-gray-400 hover:bg-white/5"}`}
            >
              {TAB_LABELS[t]}
            </button>
          ))}
        </div>
      </div>

      {tab === "players" && <PlayersTab />}
      {tab === "guilds" && <GuildsTab />}
      {tab === "resolve" && <ResolveTab />}
    </div>
  );
}

const PLAYER_COLUMNS: ColumnDef<PlayerSnapshotRow, unknown>[] = [
  {
    header: "Player",
    id: "identity",
    cell: ({ row }) => <Identity id={row.original.uuid} username={row.original.username} kind="minecraft" />,
  },
  {
    header: "Last snapshot",
    accessorKey: "last_snapshot_at",
    cell: (info) => <span className="text-gray-500">{fmtDate(info.getValue<string | null>())}</span>,
  },
];

function PlayersTab() {
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);

  const players = usePlayerSnapshots(search, offset, PAGE_SIZE);
  const rows = players.data?.players ?? [];

  const applySearch = () => {
    setSearch(searchDraft);
    setOffset(0);
  };

  return (
    <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <Panel>
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <input
            className="w-64 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            placeholder="IGN, UUID, or Discord username…"
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applySearch()}
          />
          <button onClick={applySearch} className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
            Search
          </button>
        </div>
        <DataTable columns={PLAYER_COLUMNS} data={rows} onRowClick={(p) => setSelected(p.uuid)} emptyMessage="No snapshots match" />
        {!search && (
          <div className="mt-4 flex items-center justify-between text-xs text-gray-500">
            <span>Most recently seen players</span>
            <div className="flex gap-2">
              <button
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
                className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
              >
                Prev
              </button>
              <button
                disabled={rows.length < PAGE_SIZE}
                onClick={() => setOffset(offset + PAGE_SIZE)}
                className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
              >
                Next
              </button>
            </div>
          </div>
        )}
      </Panel>
      <PlayerSnapshotPanel uuid={selected} />
    </div>
  );
}

function PlayerSnapshotPanel({ uuid }: { uuid: string | null }) {
  const [ts, setTs] = useState<string | null>(null);
  const detail = usePlayerSnapshotDetail(uuid);
  const historical = usePlayerSnapshotAt(uuid, ts);

  if (!uuid) {
    return (
      <Panel title="Snapshot">
        <div className="text-sm text-gray-500">Select a player to inspect their cached snapshot history.</div>
      </Panel>
    );
  }
  if (!detail.data) {
    return (
      <Panel title="Snapshot">
        <div className="text-sm text-gray-500">Loading…</div>
      </Panel>
    );
  }

  const shown = ts ? historical.data : detail.data.latest;

  return (
    <Panel title={detail.data.username ?? detail.data.uuid} description={`${detail.data.timestamps.length} snapshots recorded`}>
      <select
        className="mb-3 w-full rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
        value={ts ?? ""}
        onChange={(e) => setTs(e.target.value || null)}
      >
        <option value="">Latest</option>
        {detail.data.timestamps.map((t) => (
          <option key={t} value={t}>
            {fmtDate(t)}
          </option>
        ))}
      </select>
      <pre className="max-h-96 overflow-auto rounded-md bg-black/40 p-3 font-mono text-[11px] text-gray-300">
        {shown ? prettyJson(shown) : "No snapshot data"}
      </pre>
    </Panel>
  );
}

const GUILD_COLUMNS: ColumnDef<GuildRow, unknown>[] = [
  {
    header: "Guild",
    id: "guild",
    cell: ({ row }) => (
      <div>
        <div className="text-gray-100">{row.original.name}</div>
        {row.original.tag && <div className="font-mono text-xs text-gray-500">[{row.original.tag}]</div>}
      </div>
    ),
  },
  { header: "Level", accessorKey: "level", cell: (info) => fmtNum(info.getValue<number>()) },
  { header: "Members", accessorKey: "member_count", cell: (info) => fmtNum(info.getValue<number>()) },
  { header: "Experience", accessorKey: "experience", cell: (info) => fmtNum(info.getValue<number>()) },
  { header: "Updated", accessorKey: "updated_at", cell: (info) => fmtDate(info.getValue<string>()) },
];

function GuildsTab() {
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState("");
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);

  const guilds = useGuildSnapshots(search, sort, offset, PAGE_SIZE);
  const total = guilds.data?.total ?? 0;

  const applySearch = () => {
    setSearch(searchDraft);
    setOffset(0);
  };

  return (
    <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
      <Panel>
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <input
            className="w-56 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            placeholder="Guild name, tag, or ID…"
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applySearch()}
          />
          <select
            className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value);
              setOffset(0);
            }}
          >
            <option value="">Sort: updated</option>
            <option value="members">Sort: members</option>
            <option value="level">Sort: level</option>
            <option value="experience">Sort: experience</option>
          </select>
          <button onClick={applySearch} className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
            Search
          </button>
        </div>
        <DataTable
          columns={GUILD_COLUMNS}
          data={guilds.data?.guilds ?? []}
          onRowClick={(g) => setSelected(g.guild_id)}
          emptyMessage="No guilds match"
        />
        <div className="mt-4 flex items-center justify-between text-xs text-gray-500">
          <span>{total ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 guilds"}</span>
          <div className="flex gap-2">
            <button
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
            >
              Prev
            </button>
            <button
              disabled={offset + PAGE_SIZE >= total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
              className="rounded-md border border-white/10 px-2 py-1 disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      </Panel>
      <GuildSnapshotPanel guildId={selected} />
    </div>
  );
}

function GuildSnapshotPanel({ guildId }: { guildId: string | null }) {
  const [ts, setTs] = useState<string | null>(null);
  const detail = useGuildDetail(guildId);
  const historical = useGuildAt(guildId, ts);

  if (!guildId) {
    return (
      <Panel title="Snapshot">
        <div className="text-sm text-gray-500">Select a guild to inspect its cached snapshot history.</div>
      </Panel>
    );
  }
  if (!detail.data) {
    return (
      <Panel title="Snapshot">
        <div className="text-sm text-gray-500">Loading…</div>
      </Panel>
    );
  }

  const shown = ts ? historical.data : detail.data.current;

  return (
    <Panel title={detail.data.name ?? detail.data.guild_id} description={`${detail.data.timestamps.length} snapshots recorded`}>
      <select
        className="mb-3 w-full rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
        value={ts ?? ""}
        onChange={(e) => setTs(e.target.value || null)}
      >
        <option value="">Latest</option>
        {detail.data.timestamps.map((t) => (
          <option key={t} value={t}>
            {fmtDate(t)}
          </option>
        ))}
      </select>
      <pre className="max-h-96 overflow-auto rounded-md bg-black/40 p-3 font-mono text-[11px] text-gray-300">
        {shown ? prettyJson(shown) : "No snapshot data"}
      </pre>
    </Panel>
  );
}

function ResolveTab() {
  const [uuidsDraft, setUuidsDraft] = useState("");
  const [discordDraft, setDiscordDraft] = useState("");
  const [uuids, setUuids] = useState<string[]>([]);
  const [discordIds, setDiscordIds] = useState<string[]>([]);

  const resolved = useResolveIds(uuids, discordIds);

  const run = () => {
    setUuids(uuidsDraft.split(",").map((s) => s.trim()).filter(Boolean));
    setDiscordIds(discordDraft.split(",").map((s) => s.trim()).filter(Boolean));
  };

  const uuidEntries = Object.entries(resolved.data?.uuids ?? {});
  const discordEntries = Object.entries(resolved.data?.discord ?? {});

  return (
    <Panel title="Batch ID resolver" description="Resolve UUIDs and Discord IDs to cached usernames">
      <div className="flex flex-col gap-2">
        <textarea
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 font-mono text-xs"
          placeholder="Comma-separated UUIDs…"
          rows={2}
          value={uuidsDraft}
          onChange={(e) => setUuidsDraft(e.target.value)}
        />
        <textarea
          className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 font-mono text-xs"
          placeholder="Comma-separated Discord IDs…"
          rows={2}
          value={discordDraft}
          onChange={(e) => setDiscordDraft(e.target.value)}
        />
        <button onClick={run} className="w-fit rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
          Resolve
        </button>
      </div>
      {(uuidEntries.length > 0 || discordEntries.length > 0) && (
        <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div>
            <div className="mb-1 text-[11px] font-medium tracking-wide text-gray-500 uppercase">Minecraft</div>
            <div className="flex flex-col divide-y divide-white/5">
              {uuidEntries.map(([uuid, username]) => (
                <div key={uuid} className="py-1.5">
                  <Identity id={uuid} username={username} kind="minecraft" />
                </div>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1 text-[11px] font-medium tracking-wide text-gray-500 uppercase">Discord</div>
            <div className="flex flex-col divide-y divide-white/5">
              {discordEntries.map(([id, username]) => (
                <div key={id} className="py-1.5">
                  <Identity id={id} username={username} />
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </Panel>
  );
}
