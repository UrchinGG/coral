import { Fragment } from "react";
import { useNavigate } from "react-router-dom";

import type { RouteTraffic, TrafficRange } from "../../api/types/traffic";
import { Badge, MethodBadge } from "../../kit/Badges";
import { Sparkline } from "../../kit/Sparkline";
import { Table, type Column } from "../../kit/Table";
import { cx } from "../../lib/cx";
import { formatMs, formatNumber, formatPercent } from "../../lib/format";
import { rangeQuery } from "./traffic-view";

type RoutesTableProps = { routes: readonly RouteTraffic[]; scope: TrafficRange };

const ERROR_RATE_ALERT = 0.05;
const SLOW_P95_MS = 1_000;
const SPARK_HEIGHT = 18;
const UNMATCHED_CAUSE = "unmatched";
const ROUTE_SEPARATOR = "/";

const COLUMNS: readonly Column<RouteTraffic>[] = [
  {
    id: "route",
    header: "Route",
    cell: (row) => (
      <span className="inline-flex items-center gap-1.5">
        <MethodBadge method={row.method} />
        {row.route === null ? <Badge>unmatched</Badge> : <RouteText route={row.route} />}
      </span>
    ),
  },
  { id: "requests", header: "Requests", numeric: true, cell: (row) => formatNumber(row.requests) },
  {
    id: "errors",
    header: "Error %",
    numeric: true,
    cell: (row) => <span className={cx(row.error_rate > ERROR_RATE_ALERT && "text-danger")}>{formatPercent(row.error_rate)}</span>,
  },
  {
    id: "limited",
    header: "429",
    numeric: true,
    priority: 2,
    cell: (row) => <span className={cx(row.rate_limited > 0 && "text-limited")}>{formatNumber(row.rate_limited)}</span>,
  },
  { id: "p50", header: "p50", numeric: true, priority: 2, cell: (row) => latency(row.p50) },
  {
    id: "p95",
    header: "p95",
    numeric: true,
    cell: (row) => <span className={cx(row.p95 !== null && row.p95 >= SLOW_P95_MS && "text-warning")}>{latency(row.p95)}</span>,
  },
  { id: "p99", header: "p99", numeric: true, priority: 3, cell: (row) => latency(row.p99) },
  {
    id: "spark",
    header: "Trend",
    priority: 2,
    className: "w-[110px]",
    cell: (row) => <Sparkline values={row.spark} color="var(--color-accent)" height={SPARK_HEIGHT} label={`Requests over time for ${row.route ?? "unmatched paths"}`} />,
  },
];

export function RoutesTable({ routes, scope }: RoutesTableProps) {
  const navigate = useNavigate();
  const open = (row: RouteTraffic) => {
    const params = new URLSearchParams(rangeQuery(scope));
    if (row.route === null) params.set("cause", UNMATCHED_CAUSE);
    else params.set("route", row.route);
    params.set("method", row.method);
    navigate(`/requests?${params}`);
  };
  return <Table label="Traffic by route" columns={COLUMNS} rows={routes} rowKey={routeKey} onRowClick={open} empty="No requests in this scope." />;
}

function RouteText({ route }: { route: string }) {
  return (
    <span className="font-mono text-[11.5px]">
      {route.split(ROUTE_SEPARATOR).map((segment, index) => (
        <Fragment key={index}>
          {index > 0 && (
            <>
              {ROUTE_SEPARATOR}
              <wbr />
            </>
          )}
          {segment}
        </Fragment>
      ))}
    </span>
  );
}

function routeKey(row: RouteTraffic): string {
  return `${row.method} ${row.route ?? UNMATCHED_CAUSE}`;
}

function latency(ms: number | null): string {
  return ms === null ? "—" : formatMs(ms);
}
