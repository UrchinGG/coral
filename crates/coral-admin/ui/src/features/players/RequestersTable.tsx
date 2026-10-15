import { useMemo } from "react";

import type { PlayerForm, PlayerRequester } from "../../api/types/players";
import { Badge } from "../../kit/Badges";
import { EntityLink } from "../../kit/EntityLink";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { SERVICE_LABEL } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";
import { FORM_ORDER } from "./player-state";

type RequestersTableProps = {
  groups: readonly PlayerRequester[];
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
  loading: boolean;
};

const REQUESTERS_HEIGHT = "360px";

export function RequestersTable({ groups, selectedKey, onSelect, loading }: RequestersTableProps) {
  const columns = useRequesterColumns();
  return (
    <Table
      label="Requesters of this player"
      columns={columns}
      rows={groups}
      rowKey={(group) => group.key}
      selectedKey={selectedKey}
      onRowClick={(group) => onSelect(group.key === selectedKey ? null : group.key)}
      virtual
      height={REQUESTERS_HEIGHT}
      loading={loading}
      empty="No requester looked this player up in this window with the selected forms."
    />
  );
}

function useRequesterColumns(): readonly Column<PlayerRequester>[] {
  const { time } = useTimeFormat();
  return useMemo(() => {
    const stamp = (iso: string) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(iso, { seconds: false })}</span>;
    return [
      { id: "requester", header: "Requester", cell: (group) => <EntityLink entity={group.requester} /> },
      { id: "actor", header: "Actor", cell: (group) => (group.actor === null ? <span className="text-subtle">—</span> : <EntityLink entity={group.actor} />), priority: 2 },
      { id: "service", header: "Service", cell: (group) => serviceText(group), priority: 3 },
      { id: "requests", header: "Requests", cell: (group) => formatNumber(group.requests), numeric: true },
      { id: "limited", header: "429", cell: (group) => <Count value={group.rate_limited} tone="text-limited" />, numeric: true, priority: 2 },
      { id: "errors", header: "Errors", cell: (group) => <Count value={group.errors} tone="text-danger" />, numeric: true, priority: 2 },
      { id: "first", header: "First", cell: (group) => stamp(group.first), priority: 3 },
      { id: "last", header: "Last", cell: (group) => stamp(group.last), priority: 2 },
      { id: "forms", header: "Forms", cell: (group) => <FormBadges forms={group.forms} />, priority: 3 },
    ];
  }, [time]);
}

function Count({ value, tone }: { value: number; tone: string }) {
  return value === 0 ? <span className="text-subtle">0</span> : <span className={tone}>{formatNumber(value)}</span>;
}

function FormBadges({ forms }: { forms: Partial<Record<PlayerForm, number>> }) {
  return (
    <span className="inline-flex gap-1 whitespace-nowrap">
      {FORM_ORDER.filter((form) => (forms[form] ?? 0) > 0).map((form) => (
        <Badge key={form} title={`${formatNumber(forms[form] ?? 0)} lookups by ${form}`}>
          {form}
        </Badge>
      ))}
    </span>
  );
}

function serviceText(group: PlayerRequester) {
  if (group.key_kind !== "internal") return <span className="text-subtle">—</span>;
  return SERVICE_LABEL[group.service];
}
