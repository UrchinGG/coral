import { useMemo } from "react";

import type { AgentUse } from "../../api/types/accounts";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";

export function AgentTable({ agents, label }: { agents: readonly AgentUse[]; label: string }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<AgentUse>[]>(
    () => [
      { id: "agent", header: "User agent", cell: (row) => <span className="font-mono text-[11.5px] wrap-anywhere">{row.user_agent}</span> },
      { id: "requests", header: "Requests", cell: (row) => formatNumber(row.requests), numeric: true },
      { id: "first", header: "First", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.first_seen, { seconds: false, date: "always" })}</span>, priority: 2 },
      { id: "last", header: "Last", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.last_seen, { seconds: false, date: "always" })}</span> },
    ],
    [time],
  );
  return <Table label={label} columns={columns} rows={agents} rowKey={(row) => row.user_agent} height="360px" empty="No user agent recorded." />;
}
