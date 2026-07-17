import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ColumnDef } from "@tanstack/react-table";
import { useMembers, type MemberListFilters } from "../api/members";
import type { MemberSummary } from "../api/types";
import { Badge } from "../components/Badge";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
import { ModerationTabs } from "../components/ModerationTabs";
import { Panel } from "../components/Panel";
import { RecentActivity } from "../components/RecentActivity";
import { accessRankLabel, accessRankTone, fmtDate, fmtNum, fmtPercent } from "../format";

const PAGE_SIZE = 50;

const EMPTY_FILTERS: MemberListFilters = { search: "", sort: "", dir: "", rank: "", locked: false, haskey: false };

const COLUMNS: ColumnDef<MemberSummary, unknown>[] = [
  {
    header: "Member",
    id: "identity",
    cell: ({ row }) => (
      <div className="flex flex-col gap-0.5">
        <Identity id={row.original.discord_id} username={row.original.discord_username} />
        {row.original.uuid && (
          <Identity id={row.original.uuid} username={row.original.minecraft_username} kind="minecraft" />
        )}
      </div>
    ),
  },
  {
    header: "Access",
    id: "access",
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        {row.original.is_owner ? (
          <Badge label="Owner" tone="accent" />
        ) : (
          row.original.access_level > 0 && (
            <Badge label={accessRankLabel(row.original.access_level)} tone={accessRankTone(row.original.access_level)} />
          )
        )}
        {row.original.key_locked && <Badge label="Locked" tone="danger" />}
        {row.original.tagging_disabled && <Badge label="No tagging" tone="warning" />}
      </div>
    ),
  },
  {
    header: "Strikes",
    accessorKey: "strike_count",
    cell: (info) => {
      const count = info.getValue<number>();
      return count > 0 ? <span className="text-danger">{count}</span> : <span className="text-gray-600">0</span>;
    },
  },
  {
    header: "Dev key",
    accessorKey: "has_dev_key",
    cell: (info) => (info.getValue<boolean>() ? <Badge label="Yes" /> : <span className="text-gray-600">—</span>),
  },
  {
    header: "Last IP",
    accessorKey: "last_seen_ip",
    cell: (info) => <span className="font-mono text-xs">{info.getValue<string | null>() ?? "—"}</span>,
  },
  {
    header: "Budget",
    accessorKey: "budget_utilization",
    cell: (info) => {
      const v = info.getValue<number | null>();
      if (v === null || v === undefined) return <span className="text-gray-600">—</span>;
      return <span className={v > 0.85 ? "text-danger" : v > 0.6 ? "text-warning" : ""}>{fmtPercent(v)}</span>;
    },
  },
  { header: "Requests", accessorKey: "request_count", cell: (info) => fmtNum(info.getValue<number>()) },
  { header: "Joined", accessorKey: "join_date", cell: (info) => fmtDate(info.getValue<string>()) },
];

export function Members() {
  const [filters, setFilters] = useState(EMPTY_FILTERS);
  const [searchDraft, setSearchDraft] = useState("");
  const [offset, setOffset] = useState(0);
  const navigate = useNavigate();

  const members = useMembers(filters, offset, PAGE_SIZE);
  const total = members.data?.total ?? 0;

  const applySearch = () => {
    setFilters((f) => ({ ...f, search: searchDraft }));
    setOffset(0);
  };

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold text-gray-100">Members & Moderation</h1>
        <ModerationTabs active="members" />
      </div>

      <RecentActivity limit={15} />

      <Panel>
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <input
            className="w-64 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            placeholder="Search Minecraft IGN, Discord username, or ID…"
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applySearch()}
          />
          <select
            className="rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            value={filters.rank}
            onChange={(e) => {
              setFilters((f) => ({ ...f, rank: e.target.value }));
              setOffset(0);
            }}
          >
            <option value="">Any rank</option>
            <option value="2">Helper+</option>
            <option value="3">Moderator+</option>
            <option value="4">Admin+</option>
          </select>
          <label className="flex items-center gap-1 text-xs text-gray-400">
            <input
              type="checkbox"
              checked={filters.locked}
              onChange={(e) => {
                setFilters((f) => ({ ...f, locked: e.target.checked }));
                setOffset(0);
              }}
            />
            Locked
          </label>
          <label className="flex items-center gap-1 text-xs text-gray-400">
            <input
              type="checkbox"
              checked={filters.haskey}
              onChange={(e) => {
                setFilters((f) => ({ ...f, haskey: e.target.checked }));
                setOffset(0);
              }}
            />
            Has key
          </label>
          <button onClick={applySearch} className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
            Search
          </button>
        </div>
        <DataTable
          columns={COLUMNS}
          data={members.data?.members ?? []}
          onRowClick={(m) => navigate(`/members/${m.id}`)}
          emptyMessage="No members match"
        />
        <div className="mt-4 flex items-center justify-between text-xs text-gray-500">
          <span>{total ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 members"}</span>
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
    </div>
  );
}
