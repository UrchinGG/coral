import type { Cause, KeyKind } from "../../api/types/refs";
import type { CallerFilter, CallerSort, CallerTraffic, StatusClass } from "../../api/types/traffic";
import { Badge } from "../../kit/Badges";
import { CAUSE_LABEL } from "../../kit/badge-styles";
import { EntityLink } from "../../kit/EntityLink";
import { Segmented } from "../../kit/Segmented";
import { Select, type SelectOption } from "../../kit/Select";
import { Table, type Column } from "../../kit/Table";
import { refIdentity } from "../../lib/refs";
import { cx } from "../../lib/cx";
import { formatNumber } from "../../lib/format";
import { CALLER_SORTS, CAUSES, STATUS_CLASSES, type TrafficPatch } from "./traffic-view";

type CallerFiltersProps = { filter: CallerFilter; routes: readonly string[]; onChange: (patch: TrafficPatch) => void };

const ANY = "";
const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"] as const;
const UNMATCHED_ROUTE = "unmatched";

const SORT_LABEL: Record<CallerSort, string> = {
  requests: "Requests",
  rate_limited: "429s",
  errors: "Errors",
  subjects: "Players",
};

const KEY_KIND_LABEL: Record<KeyKind, string> = {
  none: "no key",
  personal: "personal key",
  developer: "developer key",
  internal: "internal key",
  session: "session",
  invalid: "invalid key",
  unverified: "unverified key",
  legacy: "legacy key",
};

const CLASS_LABEL: Record<StatusClass, string> = {
  "2xx": "2xx",
  "4xx": "4xx",
  "5xx": "5xx",
  error: "any error",
  rate_limited: "rate limited",
};

const COLUMNS: readonly Column<CallerTraffic>[] = [
  {
    id: "caller",
    header: "Caller",
    cell: (row) => (
      <span className="inline-flex flex-wrap items-center gap-1.5">
        <EntityLink entity={row.requester} />
        {row.requester.kind === "anonymous" && <Badge>{KEY_KIND_LABEL[row.key_kind]}</Badge>}
        {row.actor !== null && (
          <>
            <span className="text-subtle">for</span>
            <EntityLink entity={row.actor} />
          </>
        )}
      </span>
    ),
  },
  { id: "requests", header: "Requests", numeric: true, cell: (row) => formatNumber(row.requests) },
  { id: "subjects", header: "Players", numeric: true, priority: 2, cell: (row) => formatNumber(row.subjects) },
  {
    id: "limited",
    header: "429",
    numeric: true,
    cell: (row) => <span className={cx(row.rate_limited > 0 && "text-limited")}>{formatNumber(row.rate_limited)}</span>,
  },
  {
    id: "errors",
    header: "Errors",
    numeric: true,
    priority: 2,
    cell: (row) => <span className={cx(row.errors > 0 && "text-danger")}>{formatNumber(row.errors)}</span>,
  },
];

export function CallersTable({ callers }: { callers: readonly CallerTraffic[] }) {
  return <Table label="Top callers" columns={COLUMNS} rows={callers} rowKey={callerKey} empty="No callers match these filters in this scope." />;
}

export function CallerFilters({ filter, routes, onChange }: CallerFiltersProps) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
      <Select aria-label="Route" value={filter.route ?? ANY} options={routeOptions(routes, filter.route)} onChange={(route) => onChange({ route })} />
      <Select aria-label="Method" value={filter.method ?? ANY} options={withAny("Any method", METHODS.map((method) => ({ value: method, label: method })))} onChange={(method) => onChange({ method })} />
      <Select
        aria-label="Status class"
        value={filter.class ?? ANY}
        options={withAny("Any status", STATUS_CLASSES.map((value) => ({ value, label: CLASS_LABEL[value] })))}
        onChange={(value) => onChange({ class: value })}
      />
      <Select
        aria-label="Cause"
        value={filter.cause ?? ANY}
        options={withAny("Any cause", CAUSES.map((cause: Cause) => ({ value: cause, label: CAUSE_LABEL[cause] })))}
        onChange={(cause) => onChange({ cause })}
      />
      <Segmented
        label="Sort callers by"
        value={filter.sort}
        options={CALLER_SORTS.map((sort) => ({ value: sort, label: SORT_LABEL[sort] }))}
        onChange={(sort) => onChange({ sort: sort === "requests" ? null : sort })}
      />
    </div>
  );
}

function routeOptions(routes: readonly string[], selected: string | null): SelectOption<string>[] {
  const known = selected === null || selected === UNMATCHED_ROUTE || routes.includes(selected) ? routes : [selected, ...routes];
  return withAny("Any route", [...known.map((route) => ({ value: route, label: route })), { value: UNMATCHED_ROUTE, label: "unmatched paths" }]);
}

function withAny(label: string, options: readonly SelectOption<string>[]): SelectOption<string>[] {
  return [{ value: ANY, label }, ...options];
}

function callerKey(row: CallerTraffic): string {
  return [refIdentity(row.requester), row.actor === null ? "" : refIdentity(row.actor), row.key_kind, row.service, row.key_prefix ?? ""].join("|");
}
