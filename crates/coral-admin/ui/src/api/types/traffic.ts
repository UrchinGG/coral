import type { Cause, EntityRef, KeyKind, Service } from "./refs";

export type TrafficWindow = "1h" | "6h" | "24h" | "7d" | "30d";

export type TrafficRange = { window: TrafficWindow } | { from: string; to: string };

export type StatusClass = "2xx" | "4xx" | "5xx" | "error" | "rate_limited";

export type Classes = {
  c2xx: number;
  c4xx: number;
  c429_caller: number;
  c429_upstream: number;
  c5xx: number;
};

export type TrafficTotals = {
  requests: number;
  errors: number;
  rate_limited: { caller: number; upstream: number };
  classes: Classes;
  p50: number | null;
  p95: number | null;
  p99: number | null;
  rpm_now: number | null;
};

export type Bounds = { from: string; to: string };

export type TrafficSummary = {
  range: Bounds;
  totals: TrafficTotals;
  causes: Partial<Record<Cause, number>>;
  prev: TrafficTotals | null;
  generated_at: string;
};

export type SeriesPoint = Classes & { t: string; requests: number; p95: number | null };

export type TrafficSeries = Bounds & { step_s: number; points: SeriesPoint[]; last_partial: boolean; rolled_to: string | null };

export type RouteTraffic = {
  route: string | null;
  method: string;
  requests: number;
  errors: number;
  error_rate: number;
  rate_limited: number;
  p50: number | null;
  p95: number | null;
  p99: number | null;
  spark: number[];
};

export type CallerSort = "requests" | "rate_limited" | "errors" | "subjects";

export type CallerTraffic = {
  requester: EntityRef;
  actor: EntityRef | null;
  key_kind: KeyKind;
  service: Service;
  key_prefix: string | null;
  requests: number;
  rate_limited: number;
  errors: number;
  subjects: number;
};

export type TopCallers = Bounds & { callers: CallerTraffic[] };

export type CallerFilter = { route: string | null; method: string | null; class: StatusClass | null; cause: Cause | null; sort: CallerSort };

export type Limiter = "personal" | "developer" | "session" | "session_uuids" | "minecraft" | "minecraft_uuids" | "key_uuids" | "tagging";

export type LimitUsage = { limiter: Limiter; player: string | null; used: number; limit: number };

export type PressuredAccount = { account: EntityRef; limits: LimitUsage[] };

export type LimitPressure = {
  generated_at: string;
  threshold: number;
  window_s: number;
  candidates: number;
  accounts: PressuredAccount[];
};

export type UpstreamPoint = { t: string; requests: number; errors: number };

export type SnapshotPoint = { t: string; first: number; delta: number };

export type Capacity = { used: number; limit: number; at: string };

export type UpstreamTraffic = Bounds & {
  step_s: number;
  history_from: string;
  series: UpstreamPoint[];
  snapshots: SnapshotPoint[];
  background: { used: number; limit: number | null; window_s: number };
  capacity: Capacity | null;
};
