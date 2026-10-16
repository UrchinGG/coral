import { queryOptions } from "@tanstack/react-query";

import { keys } from "../../api/keys";
import { getQuery, listQuery, pinnedQuery } from "../../api/queryClient";
import type { RollupFilter } from "../../api/types/request-explorer";

export type HistogramPoint = {
  t: string;
  requests: number;
  c2xx: number;
  c4xx: number;
  c429_caller: number;
  c429_upstream: number;
  c5xx: number;
  p95: number | null;
};

export type HistogramSeries = { step_s: number; points: HistogramPoint[]; last_partial: boolean };

const SERIES_FIELDS = ["from", "to", "route", "class", "cause", "member", "actor", "key_kind"] as const;

export function histogramQuery(filter: RollupFilter) {
  const params = new URLSearchParams();
  for (const field of SERIES_FIELDS) {
    const value = filter[field];
    if (value !== undefined) params.set(field, String(value));
  }
  return queryOptions({
    ...listQuery,
    ...pinnedQuery,
    queryKey: keys.requests.histogram(params.toString()),
    queryFn: getQuery<HistogramSeries>(`/traffic/series?${params}`),
  });
}
