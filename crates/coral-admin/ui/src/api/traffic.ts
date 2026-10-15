import { queryOptions } from "@tanstack/react-query";

import { keys } from "./keys";
import { getQuery, listQuery, liveQuery, POLL_PULSE_MS } from "./queryClient";
import type { TrafficRange, TrafficSummary } from "./types/traffic";

export const PULSE_RANGE: TrafficRange = { window: "1h" };

export function trafficSummaryQuery(range: TrafficRange, compare = false) {
  const params = rangeParams(range);
  if (compare) params.set("compare", "1");
  return queryOptions({
    queryKey: [...keys.traffic.summary(range), compare] as const,
    queryFn: getQuery<TrafficSummary>(`/traffic/summary?${params}`),
    ...listQuery,
  });
}

export function pulseQuery() {
  return queryOptions({ ...trafficSummaryQuery(PULSE_RANGE), ...liveQuery(POLL_PULSE_MS) });
}

export function rangeParams(range: TrafficRange): URLSearchParams {
  return "window" in range ? new URLSearchParams({ window: range.window }) : new URLSearchParams({ from: range.from, to: range.to });
}
