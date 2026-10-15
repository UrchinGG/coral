import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { accountActivityQuery, accountRequestsQuery } from "../../../api/accounts";
import type { AccountActivity, AccountCore, BreakdownRow, Totals } from "../../../api/types/accounts";
import { useBoot } from "../../../app/boot";
import { MethodBadge, StatusPill } from "../../../kit/Badges";
import { CoverageNotices } from "../../../kit/CoverageNotices";
import { KpiStrip, KpiTile } from "../../../kit/KpiTile";
import { QueryContent } from "../../../kit/QueryContent";
import { Section } from "../../../kit/Section";
import { Segmented } from "../../../kit/Segmented";
import { Table, type Column } from "../../../kit/Table";
import { TimeSeriesChart, type ChartPoint, type ChartSeries } from "../../../kit/TimeSeriesChart";
import { formatNumber, formatPercent } from "../../../lib/format";
import { parseTimestamp, useTimeFormat } from "../../../lib/time";
import { useUrlEnum } from "../../../lib/url-state";
import { RequestStream } from "../../entity/RequestStream";
import { ACTIVITY_WINDOWS, DEFAULT_ACTIVITY_WINDOW } from "../vocabulary";

const SECOND_MS = 1_000;
const DAY_MS = 86_400_000;
const STAMP_DATE_LENGTH = 10;
const STAMP_MONTH_START = 5;

const SERIES: readonly ChartSeries[] = [
  { key: "served", label: "served", color: "#34d399", opacity: 0.42 },
  { key: "caller", label: "429 caller", color: "#fb923c" },
  { key: "upstream", label: "429 upstream", color: "#f472b6" },
  { key: "errors", label: "other errors", color: "#f87171" },
];

const BREAKDOWN_COLUMNS: readonly Column<BreakdownRow>[] = [
  {
    id: "route",
    header: "Route",
    cell: (row) => (
      <span className="inline-flex items-center gap-1.5">
        <MethodBadge method={row.method} />
        <span className="font-mono text-[11.5px] whitespace-nowrap">{row.route ?? "unmatched"}</span>
      </span>
    ),
  },
  { id: "status", header: "Status · cause", cell: (row) => <StatusPill status={row.status} outcomeStatus={null} cause={row.cause} /> },
  { id: "requests", header: "Requests", cell: (row) => formatNumber(row.requests), numeric: true },
];

export function ActivityTab({ core }: { core: AccountCore }) {
  const discordId = core.account.discord_id;
  const [window, setWindow] = useUrlEnum("window", ACTIVITY_WINDOWS, DEFAULT_ACTIVITY_WINDOW);
  const activity = useQuery(accountActivityQuery(discordId, window));
  const requests = useInfiniteQuery(accountRequestsQuery(discordId));
  const { coverage } = useBoot();
  return (
    <>
      <ActivityKpis h24={core.activity.h24} d14={core.activity.d14} mentions={core.activity.mentions_24h} />
      <Section
        title="Requests"
        fetching={activity.isFetching && activity.data !== undefined}
        actions={<Segmented label="Activity window" value={window} options={ACTIVITY_WINDOWS.map((value) => ({ value, label: value }))} onChange={setWindow} />}
      >
        <QueryContent query={activity}>
          {(data) => (
            <div className="flex flex-col gap-3">
              <CoverageNotices coverage={coverage} features={["rollups"]} from={data.from} to={data.to} />
              <ActivityChart activity={data} />
            </div>
          )}
        </QueryContent>
      </Section>
      <Section title="Route × status × cause" count={window} flush fetching={activity.isFetching && activity.data !== undefined}>
        <QueryContent query={activity} isEmpty={(data) => data.breakdown.length === 0} empty="No requests in this window.">
          {(data) => <Table label="Requests by route, status and cause" columns={BREAKDOWN_COLUMNS} rows={data.breakdown} rowKey={breakdownKey} height="360px" />}
        </QueryContent>
      </Section>
      <Section title="Request stream" flush fetching={requests.isFetching && !requests.isFetchingNextPage && requests.data !== undefined}>
        <RequestStream label="Requests by this account" query={requests} requester={core.member === null} empty="No requests from this account in the last 14 days." />
      </Section>
    </>
  );
}

function ActivityKpis({ h24, d14, mentions }: { h24: Totals; d14: Totals; mentions: number }) {
  return (
    <KpiStrip compact>
      <KpiTile label="Requests 24 h" value={formatNumber(h24.requests)} />
      <KpiTile label="Requests 14 d" value={formatNumber(d14.requests)} />
      <KpiTile label="429 caller 24 h" swatch="#fb923c" value={formatNumber(h24.rate_limited_caller)} sub={`${formatNumber(d14.rate_limited_caller)} in 14 d`} />
      <KpiTile label="429 upstream 24 h" swatch="#f472b6" value={formatNumber(h24.rate_limited_upstream)} sub={`${formatNumber(d14.rate_limited_upstream)} in 14 d`} />
      <KpiTile label="Error share 24 h" value={h24.requests === 0 ? "—" : formatPercent(h24.errors / h24.requests)} sub={`${formatNumber(h24.errors)} other errors`} />
      <KpiTile label="Player mentions 24 h" value={formatNumber(mentions)} />
    </KpiStrip>
  );
}

function ActivityChart({ activity }: { activity: AccountActivity }) {
  const { time } = useTimeFormat();
  const points = useMemo(() => chartPoints(activity), [activity]);
  const daily = activity.step_s * SECOND_MS >= DAY_MS;
  const multiDay = parseTimestamp(activity.to).getTime() - parseTimestamp(activity.from).getTime() > DAY_MS;
  const tick = (t: string) => {
    const stamp = time(t, { seconds: false, date: "always" });
    if (daily) return stamp.slice(0, STAMP_DATE_LENGTH);
    return multiDay ? stamp.slice(STAMP_MONTH_START) : time(t, { seconds: false, date: "never" });
  };
  return (
    <TimeSeriesChart
      label="Requests per bucket by outcome"
      points={points}
      stepMs={activity.step_s * SECOND_MS}
      series={SERIES}
      unit={daily ? "req/day" : "req/h"}
      formatTick={tick}
      formatTooltipTime={(t) => time(t, { seconds: false, date: "always", zone: true })}
    />
  );
}

function chartPoints(activity: AccountActivity): ChartPoint[] {
  return activity.series.map((point) => ({
    t: point.t,
    values: {
      served: point.requests - point.rate_limited_caller - point.rate_limited_upstream - point.errors,
      caller: point.rate_limited_caller,
      upstream: point.rate_limited_upstream,
      errors: point.errors,
    },
  }));
}

function breakdownKey(row: BreakdownRow): string {
  return `${row.method} ${row.route ?? ""} ${row.status} ${row.cause}`;
}
