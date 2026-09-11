import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ColumnDef } from "@tanstack/react-table";
import { useStarfishUsers } from "../api/starfish";
import type { StarfishUserRow } from "../api/types";
import { Badge } from "../components/Badge";
import { DataTable } from "../components/DataTable";
import { Identity } from "../components/Identity";
import { Panel } from "../components/Panel";
import { StarfishSubNav } from "../components/StarfishSubNav";
import { fmtDate, fmtNum } from "../format";

const PAGE_SIZE = 50;

const LICENSE_TONE: Record<string, "ok" | "danger" | "default"> = {
  active: "ok",
  suspended: "danger",
  inactive: "default",
};

const COLUMNS: ColumnDef<StarfishUserRow, unknown>[] = [
  {
    header: "User",
    id: "user",
    cell: ({ row }) => (
      <Identity
        id={row.original.discord_id}
        username={row.original.discord_username}
        linkTo={row.original.member_id ? `/members/${row.original.member_id}` : undefined}
      />
    ),
  },
  {
    header: "License",
    id: "license",
    cell: ({ row }) => <Badge label={row.original.license_status} tone={LICENSE_TONE[row.original.license_status] ?? "default"} />,
  },
  {
    header: "Session",
    id: "session",
    cell: ({ row }) =>
      row.original.has_active_session ? <Badge label="Active" tone="ok" /> : <span className="text-gray-600">—</span>,
  },
  { header: "HWIDs", id: "hwids", cell: ({ row }) => fmtNum(row.original.hwid_count) },
  {
    header: "Plugins",
    id: "plugins",
    cell: ({ row }) => (
      <span>
        {fmtNum(row.original.plugins_installed)} installed
        {row.original.plugins_owned > 0 && <span className="text-gray-500"> · {fmtNum(row.original.plugins_owned)} published</span>}
      </span>
    ),
  },
  { header: "Last heartbeat", id: "last_heartbeat", cell: ({ row }) => fmtDate(row.original.last_heartbeat_at) },
];

export function Starfish() {
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const navigate = useNavigate();

  const users = useStarfishUsers(search, offset, PAGE_SIZE);
  const total = users.data?.total ?? 0;

  const applySearch = () => {
    setSearch(searchDraft);
    setOffset(0);
  };

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-lg font-semibold text-gray-100">Starfish</h1>
      <StarfishSubNav />

      <Panel>
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <input
            className="w-64 rounded-md border border-white/10 bg-black/30 px-2 py-1.5 text-xs"
            placeholder="Search Discord ID or username…"
            value={searchDraft}
            onChange={(e) => setSearchDraft(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && applySearch()}
          />
          <button onClick={applySearch} className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/25">
            Search
          </button>
        </div>
        <DataTable
          columns={COLUMNS}
          data={users.data?.users ?? []}
          onRowClick={(u) => navigate(`/starfish/users/${u.id}`)}
          emptyMessage="No licensed users match"
        />
        <div className="mt-4 flex items-center justify-between text-xs text-gray-500">
          <span>{total ? `${offset + 1}–${Math.min(offset + PAGE_SIZE, total)} of ${fmtNum(total)}` : "0 users"}</span>
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
