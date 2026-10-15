import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { GitCompareArrows } from "lucide-react";

import { trafficSummaryQuery } from "../../api/traffic";
import { callersQuery, pressureQuery, routesQuery, seriesQuery, upstreamQuery } from "../../api/traffic-panels";
import type { TrafficWindow } from "../../api/types/traffic";
import { useSession } from "../../app/boot";
import { Button } from "../../kit/Button";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { Skeleton, SkeletonRows } from "../../kit/States";
import type { ChartRange } from "../../kit/TimeSeriesChart";
import { TimeRangePicker, type TimeRange } from "../../kit/TimeRangePicker";
import { useTimeFormat } from "../../lib/time";
import { CallerFilters, CallersTable } from "./CallersTable";
import { PressurePanel } from "./PressurePanel";
import { RoutesTable } from "./RoutesTable";
import { TrafficChart } from "./TrafficChart";
import { TrafficKpis } from "./TrafficKpis";
import { compareFlag, DEFAULT_WINDOW, TRAFFIC_WINDOWS, useTrafficView, type TrafficView } from "./traffic-view";
import { UpstreamPanel } from "./UpstreamPanel";

const ROLLUP_FEATURES = ["rollups"] as const;

export function TrafficPage() {
  const [view, update] = useTrafficView();
  const coverage = useSession().coverage;
  const summary = useQuery(trafficSummaryQuery(view.scope, view.compare));
  const series = useQuery(seriesQuery(view.chart));
  const routes = useQuery(routesQuery(view.scope));
  const callers = useQuery(callersQuery(view.scope, view.callers));
  const pressure = useQuery(pressureQuery());
  const upstream = useQuery(upstreamQuery(view.scope));

  const onRange = (range: TimeRange<TrafficWindow>) =>
    range.kind === "window" ? update({ window: range.window, from: null, to: null }) : update({ window: null, from: range.from, to: range.to });
  const onBrush = (range: ChartRange | null) => {
    if (view.window === null) {
      if (range !== null) update({ from: range.from, to: range.to });
      return;
    }
    update({ window: view.window, from: range?.from ?? null, to: range?.to ?? null });
  };

  return (
    <>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h1 className="text-22 font-semibold">Traffic</h1>
          <p className="m-0 text-muted">Rollup-backed. Drag across the chart to scope every table below; click a route to open it in Requests.</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <TimeRangePicker label="Window" value={pickerValue(view)} windows={TRAFFIC_WINDOWS} defaultWindow={DEFAULT_WINDOW} onChange={onRange} />
          <Button
            aria-pressed={view.compare}
            variant={view.compare ? "primary" : "default"}
            icon={<GitCompareArrows className="size-3" />}
            onClick={() => update({ compare: compareFlag(!view.compare) })}
          >
            Compare with the period before
          </Button>
        </div>
      </div>
      {summary.data !== undefined && <CoverageNotices coverage={coverage} features={ROLLUP_FEATURES} from={summary.data.range.from} to={summary.data.range.to} />}
      <QueryContent query={summary} skeleton={<Skeleton className="h-[84px] rounded-md" />}>
        {(data) => <TrafficKpis summary={data} />}
      </QueryContent>
      <Section title="Requests by status class" fetching={isRefetching(series)}>
        <QueryContent query={series} skeleton={<Skeleton className="h-[300px] rounded-md" />}>
          {(data) => <TrafficChart series={data} selection={view.selection} keepsSelection={view.window !== null} onBrush={onBrush} />}
        </QueryContent>
      </Section>
      <div className="grid gap-4 2xl:grid-cols-2">
        <Section title="Routes" count="in scope" flush fetching={isRefetching(routes)}>
          <QueryContent query={routes} skeleton={<SkeletonRows />}>
            {(data) => <RoutesTable routes={data} scope={view.scope} />}
          </QueryContent>
        </Section>
        <Section title="Top callers" count="in scope" flush fetching={isRefetching(callers)}>
          <CallerFilters filter={view.callers} routes={knownRoutes(routes.data)} onChange={update} />
          <QueryContent query={callers} skeleton={<SkeletonRows />}>
            {(data) => (
              <>
                <CallersTable callers={data.callers} />
                <CallerHours from={data.from} to={data.to} />
              </>
            )}
          </QueryContent>
        </Section>
      </div>
      <div className="grid gap-4 xl:grid-cols-2">
        <Section title="Limit pressure now" count="callers of the last 5 minutes" flush fetching={isRefetching(pressure)} actions={<span className="text-11 text-subtle">polled every 10 s</span>}>
          <QueryContent query={pressure} skeleton={<SkeletonRows count={3} />}>
            {(data) => <PressurePanel pressure={data} />}
          </QueryContent>
        </Section>
        <Section title="Upstream" count="in scope" fetching={isRefetching(upstream)}>
          <QueryContent query={upstream}>{(data) => <UpstreamPanel upstream={data} />}</QueryContent>
        </Section>
      </div>
    </>
  );
}

function CallerHours({ from, to }: { from: string; to: string }) {
  const { time } = useTimeFormat();
  return (
    <p className="m-0 border-t border-border px-3.5 py-2 text-11 text-subtle">
      Callers come from hourly rollups, so the scope widens to whole hours: {time(from, { seconds: false })} → {time(to, { seconds: false, zone: true })}.
    </p>
  );
}

function pickerValue(view: TrafficView): TimeRange<TrafficWindow> {
  if (view.window !== null) return { kind: "window", window: view.window };
  return "window" in view.chart ? { kind: "window", window: view.chart.window } : { kind: "custom", from: view.chart.from, to: view.chart.to };
}

function knownRoutes(routes: readonly { route: string | null }[] | undefined): string[] {
  return (routes ?? []).flatMap((row) => (row.route === null ? [] : [row.route])).filter((route, index, all) => all.indexOf(route) === index);
}

function isRefetching(query: UseQueryResult<unknown, Error>): boolean {
  return query.isFetching && query.data !== undefined;
}
