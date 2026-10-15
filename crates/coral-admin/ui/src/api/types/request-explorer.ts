import type { Cause, EntityRef, KeyKind, Labels, Service } from "./refs";

export type StatusClass = "2xx" | "4xx" | "5xx" | "error" | "rate_limited";

export type GroupSort = "requests" | "rate_limited" | "errors" | "subjects";

export type RollupFilter = {
  from?: string;
  to?: string;
  route?: string;
  method?: string;
  status?: number;
  class?: StatusClass;
  cause?: Cause;
  key_kind?: KeyKind;
  member?: number;
  actor?: string;
  key_prefix?: string;
};

export type RequestQuery = RollupFilter & {
  cursor?: string;
  after?: string;
  limit?: number;
  ip?: string;
  country?: string;
  player?: string;
  include_display?: boolean;
  q?: string;
};

export type GroupQuery = RollupFilter & { sort?: GroupSort; limit?: number };

export type RequesterGroup = {
  requester: EntityRef;
  actor: EntityRef | null;
  key_kind: KeyKind;
  service: Service;
  key_prefix: string | null;
  requests: number;
  rate_limited: number;
  errors: number;
  subjects: number;
  first: string;
  last: string;
};

export type GroupPage = {
  from: string;
  to: string;
  clamped_from: string | null;
  groups: RequesterGroup[];
  truncated: boolean;
  labels: Labels;
};

export type RouteCount = { route: string; method: string | null; requests_24h: number };
