import { queryOptions } from "@tanstack/react-query";

import { keys } from "./keys";
import { getQuery, listQuery, liveQuery, POLL_LIMITS_MS, POLL_PULSE_MS } from "./queryClient";
import { PULSE_RANGE, rangeParams } from "./traffic";
import type { CallerFilter, LimitPressure, RouteTraffic, TopCallers, TrafficRange, TrafficSeries, UpstreamTraffic } from "./types/traffic";

const TOP_CALLERS = 20;

const panelKeys = {
  series: (range: TrafficRange) => [...keys.traffic.all(), "series", range] as const,
  routes: (range: TrafficRange) => [...keys.traffic.all(), "routes", range] as const,
  callers: (range: TrafficRange, filter: CallerFilter) => [...keys.traffic.all(), "callers", range, filter] as const,
  pressure: () => [...keys.traffic.all(), "pressure"] as const,
  upstream: (range: TrafficRange) => [...keys.traffic.all(), "upstream", range] as const,
};

export function seriesQuery(range: TrafficRange) {
  return queryOptions({
    queryKey: panelKeys.series(range),
    queryFn: getQuery<TrafficSeries>(`/traffic/series?${rangeParams(range)}`),
    ...listQuery,
  });
}

export function routesQuery(range: TrafficRange) {
  return queryOptions({
    queryKey: panelKeys.routes(range),
    queryFn: getQuery<RouteTraffic[]>(`/traffic/routes?${rangeParams(range)}`),
    ...listQuery,
  });
}

export function callersQuery(range: TrafficRange, filter: CallerFilter) {
  return queryOptions({
    queryKey: panelKeys.callers(range, filter),
    queryFn: getQuery<TopCallers>(`/traffic/callers?${callerParams(range, filter)}`),
    ...listQuery,
  });
}

export function pressureQuery() {
  return queryOptions({
    queryKey: panelKeys.pressure(),
    queryFn: getQuery<LimitPressure>("/traffic/pressure"),
    ...liveQuery(POLL_LIMITS_MS),
  });
}

export function upstreamQuery(range: TrafficRange) {
  return queryOptions({
    queryKey: panelKeys.upstream(range),
    queryFn: getQuery<UpstreamTraffic>(`/traffic/upstream?${rangeParams(range)}`),
    ...listQuery,
  });
}

export function pulseSeriesQuery() {
  return queryOptions({ ...seriesQuery(PULSE_RANGE), ...liveQuery(POLL_PULSE_MS) });
}

export function pulseUpstreamQuery() {
  return queryOptions({ ...upstreamQuery(PULSE_RANGE), ...liveQuery(POLL_PULSE_MS) });
}

function callerParams(range: TrafficRange, filter: CallerFilter): URLSearchParams {
  const params = rangeParams(range);
  for (const name of ["route", "method", "class", "cause"] as const) {
    const value = filter[name];
    if (value !== null) params.set(name, value);
  }
  params.set("sort", filter.sort);
  params.set("limit", String(TOP_CALLERS));
  return params;
}
