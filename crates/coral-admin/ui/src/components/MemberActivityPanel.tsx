import { useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { useRequestLog } from "../api/diagnostics";
import type { RequestRow } from "../api/types";
import { fmtDate, fmtMs } from "../format";
import { ActivityLink } from "./ActivityLink";
import { DataTable } from "./DataTable";

const COLUMNS: ColumnDef<RequestRow, unknown>[] = [
  { header: "Time", accessorKey: "ts", cell: (info) => <span className="whitespace-nowrap text-gray-500">{fmtDate(info.getValue<string>())}</span> },
  { header: "Method", accessorKey: "method", cell: (info) => info.getValue<string | null>() ?? "—" },
  {
    header: "Path",
    accessorKey: "path",
    cell: (info) => <span className="font-mono text-xs">{info.getValue<string | null>() ?? "—"}</span>,
  },
  { header: "Status", accessorKey: "status", cell: (info) => info.getValue<number | null>() ?? "—" },
  { header: "Latency", accessorKey: "latency_ms", cell: (info) => <span className="text-gray-500">{fmtMs(info.getValue<number | null>())}</span> },
];

export function MemberActivityPanel({ discordId }: { discordId: string }) {
  const [hours] = useState(24 * 7);
  const log = useRequestLog({ hours, discord_id: discordId }, 0, 20);

  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="mb-2 flex items-center justify-between">
        <div className="text-sm font-medium text-gray-300">Recent API activity (7d)</div>
        <ActivityLink discordId={discordId} hours={168} label="Full diagnostics log →" />
      </div>
      <DataTable columns={COLUMNS} data={log.data?.requests ?? []} emptyMessage="No requests in this window" />
      {log.data && log.data.total > 20 && (
        <div className="mt-2 text-xs text-gray-500">{log.data.total.toLocaleString()} total in this window</div>
      )}
    </div>
  );
}
