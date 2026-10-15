import { useInfiniteQuery } from "@tanstack/react-query";

import { strikesQuery } from "../../api/moderation";
import type { StrikeFilter, StrikeRow, StrikeState } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { ClampText } from "../../kit/ClampText";
import { EntityLink } from "../../kit/EntityLink";
import { Segmented } from "../../kit/Segmented";
import type { Column } from "../../kit/Table";
import type { Tone } from "../../kit/tone";
import { useUrlEnum } from "../../lib/url-state";
import { PagedTable } from "../platform/PagedTable";
import { When } from "../platform/When";
import { STRIKE_FILTERS } from "./params";

const FILTER_LABEL: Record<StrikeFilter, string> = { active: "Active", all: "All, including expired and revoked" };
const STATE_TONE: Record<StrikeState, Tone> = { active: "danger", expired: "neutral", revoked: "warning" };

const COLUMNS: readonly Column<StrikeRow>[] = [
  { id: "account", header: "Account", cell: (row) => <EntityLink entity={row.account} tab="moderation" /> },
  { id: "state", header: "State", cell: (row) => <Pill tone={STATE_TONE[row.state]}>{row.state}</Pill> },
  { id: "reason", header: "Reason", cell: (row) => <ClampText>{row.reason}</ClampText>, className: "min-w-[220px]" },
  { id: "by", header: "Struck by", cell: (row) => <EntityLink entity={row.struck_by} />, priority: 2 },
  { id: "issued", header: "Issued", cell: (row) => <When at={row.issued_at} /> },
  { id: "expires", header: "Expires", cell: (row) => <When at={row.expires_at} empty="never" />, priority: 2 },
  { id: "revoked", header: "Revoked", cell: (row) => <RevokedCell row={row} />, priority: 2 },
  { id: "source", header: "Source", cell: (row) => (row.source_ref === null ? <span className="text-subtle">—</span> : <span className="font-mono text-12 wrap-anywhere">{row.source_ref}</span>), priority: 3 },
];

export function StrikesTab() {
  const [state, setState] = useUrlEnum("state", STRIKE_FILTERS, "active");
  const strikes = useInfiniteQuery(strikesQuery(state));
  return (
    <>
      <Segmented label="Strikes shown" value={state} options={STRIKE_FILTERS.map((value) => ({ value, label: FILTER_LABEL[value] }))} onChange={setState} className="self-start" />
      <PagedTable
        title={state === "active" ? "Active strikes" : "Every strike"}
        label="Strikes"
        query={strikes}
        columns={COLUMNS}
        rowKey={(row) => String(row.id)}
        empty={state === "active" ? "No account has an active strike." : "No strike has ever been issued."}
      />
    </>
  );
}

function RevokedCell({ row }: { row: StrikeRow }) {
  if (row.revoked_at === null) return <span className="text-subtle">—</span>;
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      <When at={row.revoked_at} />
      {row.revoked_by !== null && <EntityLink entity={row.revoked_by} />}
    </span>
  );
}
