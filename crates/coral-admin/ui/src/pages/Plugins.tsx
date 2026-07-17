import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ColumnDef } from "@tanstack/react-table";
import { usePlugins } from "../api/plugins";
import type { PluginSummaryRow } from "../api/types";
import { Badge } from "../components/Badge";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
import { fmtDate, fmtNum } from "../format";

const PAGE_SIZE = 50;

const COLUMNS: ColumnDef<PluginSummaryRow, unknown>[] = [
  {
    header: "Plugin",
    id: "plugin",
    cell: ({ row }) => (
      <div>
        <div className="text-gray-100">{row.original.display_name}</div>
        <div className="font-mono text-xs text-gray-500">{row.original.slug}</div>
      </div>
    ),
  },
  {
    header: "Status",
    id: "status",
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        {row.original.official && <Badge label="Official" tone="ok" />}
        {row.original.unlisted && <Badge label="Unlisted" tone="warning" />}
        {row.original.disabled && <Badge label="Disabled" tone="danger" />}
        {!row.original.official && !row.original.unlisted && !row.original.disabled && (
          <span className="text-gray-600">—</span>
        )}
      </div>
    ),
  },
  {
    header: "Owner",
    id: "owner",
    cell: ({ row }) => (
      <Identity
        id={row.original.owner_discord_id}
        username={row.original.owner_discord_username}
        linkTo={row.original.owner_member_id ? `/members/${row.original.owner_member_id}` : undefined}
      />
    ),
  },
  {
    header: "Installs",
    id: "installs",
    cell: ({ row }) => (
      <span>
        {fmtNum(row.original.installs_30d)} <span className="text-gray-500">/ {fmtNum(row.original.installs_total)}</span>
      </span>
    ),
  },
  {
    header: "Rating",
    id: "rating",
    cell: ({ row }) =>
      row.original.rating_count > 0 ? (
        <span>
          {row.original.rating_bayesian.toFixed(2)} <span className="text-gray-500">({row.original.rating_count})</span>
        </span>
      ) : (
        <span className="text-gray-600">—</span>
      ),
  },
  { header: "Updated", accessorKey: "updated_at", cell: (info) => fmtDate(info.getValue<string>()) },
];

export function Plugins() {
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const navigate = useNavigate();

  const plugins = usePlugins(search, offset, PAGE_SIZE);
  const total = plugins.data?.total ?? 0;

  const applySearch = () => {
    setSearch(searchDraft);
    setOffset(0);
  };

  return (
    <div className="flex flex-col gap-5">
      <h1 className="text-lg font-semibold">Plugins</h1>

      <div className="rounded-lg border border-white/10 bg-white/5 p-4">
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <input
            className="w-64 rounded border border-white/10 bg-black/30 px-2 py-1 text-sm"
            placeholder="Search name, slug, or description…"
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
          data={plugins.data?.plugins ?? []}
          onRowClick={(p) => navigate(`/plugins/${p.slug}`)}
          emptyMessage="No plugins match"
        />
        <div className="mt-3 flex items-center justify-between text-xs text-gray-500">
          <span>{total ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 plugins"}</span>
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
