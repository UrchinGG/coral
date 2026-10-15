import { useQuery } from "@tanstack/react-query";
import { ListFilter } from "lucide-react";

import { requestGroupsQuery } from "../../api/requests";
import type { GroupQuery, GroupSort, RequesterGroup } from "../../api/types/request-explorer";
import type { Coverage } from "../../api/types/session";
import { Button } from "../../kit/Button";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { SERVICE_LABEL, refIdentity } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";

type GroupsViewProps = {
  query: GroupQuery;
  coverage: Coverage;
  unsupported: readonly string[];
  onSortChange: (sort: GroupSort) => void;
  onPick: (group: RequesterGroup) => void;
};

const SORT_OPTIONS: { value: GroupSort; label: string }[] = [
  { value: "requests", label: "Requests" },
  { value: "rate_limited", label: "429s" },
  { value: "errors", label: "Errors" },
  { value: "subjects", label: "Subjects" },
];
const GROUP_TABLE_HEIGHT = "min(70vh, 720px)";

export function GroupsView({ query, coverage, unsupported, onSortChange, onPick }: GroupsViewProps) {
  const groups = useQuery({ ...requestGroupsQuery(query), enabled: unsupported.length === 0 });
  const columns = useGroupColumns(onPick);

  if (unsupported.length > 0) {
    return (
      <Section title="Requester groups">
        <p className="m-0 text-12 text-muted">Requester groups come from the hourly caller rollup, which does not carry {unsupported.join(", ")} filters. Clear them or switch to the list.</p>
      </Section>
    );
  }

  return (
    <Section
      title="Requester groups"
      count={groups.data === undefined ? undefined : formatNumber(groups.data.groups.length)}
      flush
      fetching={groups.isFetching && groups.data !== undefined}
      actions={<Segmented label="Sort groups by" value={query.sort ?? "requests"} options={SORT_OPTIONS} onChange={onSortChange} />}
    >
      <QueryContent query={groups} isEmpty={(page) => page.groups.length === 0} empty="No requests match these filters in this window.">
        {(page) => (
          <div className="flex flex-col">
            <div className="flex flex-col gap-1.5 px-3.5 pt-3 empty:hidden">
              <CoverageNotices coverage={coverage} features={["rollups", "body_subjects"]} from={page.from} to={page.to} />
              <WindowNote from={page.from} to={page.to} truncated={page.truncated} shown={page.groups.length} />
            </div>
            <Table label="Requester groups" columns={columns} rows={page.groups} rowKey={groupKey} onRowClick={onPick} height={GROUP_TABLE_HEIGHT} />
          </div>
        )}
      </QueryContent>
    </Section>
  );
}

function WindowNote({ from, to, truncated, shown }: { from: string; to: string; truncated: boolean; shown: number }) {
  const { time } = useTimeFormat();
  const span = `${time(from, { seconds: false, date: "always" })} → ${time(to, { seconds: false, date: "always", zone: true })}`;
  return (
    <p className="m-0 text-12 text-subtle">
      Whole hours {span}
      {truncated ? `; the top ${formatNumber(shown)} groups by the chosen column.` : "."}
    </p>
  );
}

function useGroupColumns(onPick: (group: RequesterGroup) => void): Column<RequesterGroup>[] {
  const { time } = useTimeFormat();
  return [
    { id: "requester", header: "Requester", cell: (group) => <EntityLink entity={group.requester} /> },
    { id: "actor", header: "Actor", cell: (group) => (group.actor === null ? <span className="text-subtle">—</span> : <EntityLink entity={group.actor} />), priority: 2 },
    { id: "kind", header: "Key kind", cell: (group) => <span className="font-mono text-12">{group.key_kind}</span>, priority: 2 },
    { id: "service", header: "Service", cell: (group) => (group.key_kind === "internal" ? SERVICE_LABEL[group.service] : <span className="text-subtle">—</span>), priority: 3 },
    { id: "requests", header: "Requests", cell: (group) => formatNumber(group.requests), numeric: true },
    { id: "limited", header: "429", cell: (group) => formatNumber(group.rate_limited), numeric: true },
    { id: "errors", header: "Errors", cell: (group) => formatNumber(group.errors), numeric: true, priority: 2 },
    { id: "subjects", header: "Subjects", cell: (group) => formatNumber(group.subjects), numeric: true, priority: 2 },
    { id: "first", header: "First hour", cell: (group) => <span className="font-mono text-12">{time(group.first, { seconds: false })}</span>, priority: 3 },
    { id: "last", header: "Last hour", cell: (group) => <span className="font-mono text-12">{time(group.last, { seconds: false })}</span>, priority: 2 },
    {
      id: "pick",
      header: "",
      cell: (group) => (
        <Button size="sm" icon={<ListFilter className="size-3" />} onClick={() => onPick(group)}>
          Filter list
        </Button>
      ),
    },
  ];
}

function groupKey(group: RequesterGroup): string {
  return [refIdentity(group.requester), group.actor === null ? "" : refIdentity(group.actor), group.key_kind, group.service, group.key_prefix ?? ""].join("|");
}
