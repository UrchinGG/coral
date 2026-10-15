import type { Classes, SeriesPoint, TrafficSeries } from "../../api/types/traffic";
import type { ChartSeries } from "../../kit/TimeSeriesChart";
import { parseTimestamp } from "../../lib/time";

export type ClassKey = keyof Classes;

export type ClassMeta = { key: ClassKey; label: string; color: string; badWhenUp: boolean; opacity?: number };

export const CLASS_BY_KEY: Readonly<Record<ClassKey, ClassMeta>> = {
  c2xx: { key: "c2xx", label: "2xx", color: "var(--color-ok)", badWhenUp: false, opacity: 0.42 },
  c4xx: { key: "c4xx", label: "4xx", color: "var(--color-warning)", badWhenUp: true },
  c429_caller: { key: "c429_caller", label: "429 caller", color: "var(--color-limited)", badWhenUp: true },
  c429_upstream: { key: "c429_upstream", label: "429 upstream", color: "var(--color-upstream)", badWhenUp: true },
  c5xx: { key: "c5xx", label: "5xx", color: "var(--color-danger)", badWhenUp: true },
};

export const CLASS_KEYS: readonly ClassKey[] = ["c2xx", "c4xx", "c429_caller", "c429_upstream", "c5xx"];

export const CLASS_META: readonly ClassMeta[] = CLASS_KEYS.map((key) => CLASS_BY_KEY[key]);

export const CLASS_SERIES: readonly ChartSeries[] = CLASS_META.map(({ key, label, color, opacity }) => ({ key, label, color, opacity }));

export const P95_LINE = { label: "p95 latency", unit: "ms", color: "var(--color-chart-line)" } as const;

export const SECONDS_PER_MINUTE = 60;

const MS_PER_SECOND = 1_000;

export function shareOf(part: number, total: number): number | null {
  return total > 0 ? part / total : null;
}

export function changeOf(current: number, previous: number): number | null {
  return previous > 0 ? (current - previous) / previous : null;
}

export function rolledPoints(series: TrafficSeries): SeriesPoint[] {
  if (series.rolled_to === null) return [];
  const rolledTo = parseTimestamp(series.rolled_to).getTime();
  const stepMs = series.step_s * MS_PER_SECOND;
  return series.points.filter((point) => parseTimestamp(point.t).getTime() + stepMs <= rolledTo);
}
