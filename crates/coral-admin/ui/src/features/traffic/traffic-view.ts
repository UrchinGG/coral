import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";

import type { Cause } from "../../api/types/refs";
import type { CallerFilter, CallerSort, StatusClass, TrafficRange, TrafficWindow } from "../../api/types/traffic";
import type { ChartRange } from "../../kit/TimeSeriesChart";
import { currentRouterState, flushHistoryWrites } from "../../lib/history-state";

export type TrafficView = {
  window: TrafficWindow | null;
  chart: TrafficRange;
  scope: TrafficRange;
  selection: ChartRange | null;
  compare: boolean;
  callers: CallerFilter;
};

export type TrafficPatch = Partial<Record<TrafficParam, string | null>>;

type TrafficParam = "window" | "from" | "to" | "compare" | "route" | "method" | "class" | "cause" | "sort";

export const TRAFFIC_WINDOWS: readonly TrafficWindow[] = ["1h", "6h", "24h", "7d", "30d"];
export const DEFAULT_WINDOW: TrafficWindow = "6h";
export const STATUS_CLASSES: readonly StatusClass[] = ["2xx", "4xx", "5xx", "error", "rate_limited"];
export const CALLER_SORTS: readonly CallerSort[] = ["requests", "rate_limited", "errors", "subjects"];
export const CAUSES: readonly Cause[] = [
  "ok",
  "caller_limit",
  "upstream_limit",
  "upstream_error",
  "auth",
  "validation",
  "not_found",
  "internal",
  "unmatched",
];

const COMPARE_ON = "1";

export function trafficViewOf(params: URLSearchParams): TrafficView {
  const window = oneOf(params.get("window"), TRAFFIC_WINDOWS);
  const bounds = boundsOf(params);
  const custom = window === null && bounds !== null;
  const chart: TrafficRange = custom ? bounds : { window: window ?? DEFAULT_WINDOW };
  const selection = custom ? null : bounds;
  return {
    window: custom ? null : (window ?? DEFAULT_WINDOW),
    chart,
    scope: selection ?? chart,
    selection,
    compare: params.get("compare") === COMPARE_ON,
    callers: {
      route: params.get("route"),
      method: params.get("method"),
      class: oneOf(params.get("class"), STATUS_CLASSES),
      cause: oneOf(params.get("cause"), CAUSES),
      sort: oneOf(params.get("sort"), CALLER_SORTS) ?? "requests",
    },
  };
}

export function useTrafficView(): readonly [TrafficView, (patch: TrafficPatch) => void] {
  const [params, setParams] = useSearchParams();
  const view = useMemo(() => trafficViewOf(params), [params]);
  const update = useCallback(
    (patch: TrafficPatch) => {
      flushHistoryWrites();
      setParams(
        (previous) => {
          const next = new URLSearchParams(previous);
          for (const [name, value] of Object.entries(patch)) {
            if (value === null || value === undefined || value === "") next.delete(name);
            else next.set(name, value);
          }
          return next;
        },
        { replace: true, state: currentRouterState() },
      );
    },
    [setParams],
  );
  return [view, update] as const;
}

export function compareFlag(on: boolean): string | null {
  return on ? COMPARE_ON : null;
}

export function rangeQuery(range: TrafficRange): Record<string, string> {
  return "window" in range ? { window: range.window } : { from: range.from, to: range.to };
}

function boundsOf(params: URLSearchParams): { from: string; to: string } | null {
  const from = params.get("from");
  const to = params.get("to");
  if (from === null || to === null) return null;
  const start = Date.parse(from);
  const end = Date.parse(to);
  return Number.isNaN(start) || Number.isNaN(end) || start >= end ? null : { from, to };
}

function oneOf<T extends string>(value: string | null, options: readonly T[]): T | null {
  return options.find((option) => option === value) ?? null;
}
