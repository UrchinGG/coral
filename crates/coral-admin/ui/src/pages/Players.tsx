import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ColumnDef } from "@tanstack/react-table";
import { usePlayers, type PlayerListFilters } from "../api/players";
import type { PlayerSummary } from "../api/types";
import { Badge } from "../components/Badge";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
import { ModerationTabs } from "../components/ModerationTabs";
import { fmtNum } from "../format";

const PAGE_SIZE = 50;

const TAG_TONES: Record<string, "danger" | "warning" | "default"> = {
  sniper: "danger",
  blatant_cheater: "warning",
  closet_cheater: "warning",
  confirmed_cheater: "danger",
};

const EMPTY_FILTERS: PlayerListFilters = { search: "", field: "", tag_type: "", dir: "" };

const COLUMNS: ColumnDef<PlayerSummary, unknown>[] = [
  {
    header: "Player",
    id: "identity",
    cell: ({ row }) => <Identity id={row.original.uuid} username={row.original.minecraft_username} kind="minecraft" />,
  },
  {
    header: "Tags",
    id: "tags",
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        {row.original.tags.length === 0 ? (
          <span className="text-gray-600">—</span>
        ) : (
          row.original.tags.map((t) => <Badge key={t.id} label={t.tag_type} tone={TAG_TONES[t.tag_type] ?? "default"} />)
        )}
      </div>
    ),
  },
  {
    header: "Status",
    id: "status",
    cell: ({ row }) => (row.original.is_locked ? <Badge label="Locked" tone="danger" /> : <span className="text-gray-600">—</span>),
  },
];

export function Players() {
  const [filters, setFilters] = useState(EMPTY_FILTERS);
  const [searchDraft, setSearchDraft] = useState("");
  const [offset, setOffset] = useState(0);
  const navigate = useNavigate();

  const players = usePlayers(filters, offset, PAGE_SIZE);
  const total = players.data?.total ?? 0;

  const applySearch = () => {
    setFilters((f) => ({ ...f, search: searchDraft }));
    setOffset(0);
  };

  return (
    <div className="flex flex-col gap-5">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold">Members & Moderation</h1>
        <ModerationTabs active="players" />
      </div>

      <div className="rounded-lg border border-white/10 bg-white/5 p-4">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <select
            className="rounded border border-white/10 bg-black/30 px-2 py-1 text-sm"
            value={filters.field}
            onChange={(e) => setFilters((f) => ({ ...f, field: e.target.value }))}
          >
            <option value="">By IGN or UUID</option>
            <option value="tagger">By tagger (Discord ID)</option>
            <option value="reason">By reason</option>
          </select>
          <input
            className="w-64 rounded border border-white/10 bg-black/30 px-2 py-1 text-sm"
            placeholder="Search…"
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applySearch()}
          />
          <button onClick={applySearch} className="rounded border border-white/10 px-3 py-1 text-xs hover:bg-white/10">
            Search
          </button>
        </div>
        <DataTable
          columns={COLUMNS}
          data={players.data?.players ?? []}
          onRowClick={(p) => navigate(`/players/${p.uuid}`)}
          emptyMessage="No players match"
        />
        <div className="mt-3 flex items-center justify-between text-xs text-gray-500">
          <span>{total ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 players"}</span>
          <div className="flex gap-2">
            <button
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              className="rounded border border-white/10 px-2 py-1 disabled:opacity-40"
            >
              Prev
            </button>
            <button
              disabled={offset + PAGE_SIZE >= total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
              className="rounded border border-white/10 px-2 py-1 disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
