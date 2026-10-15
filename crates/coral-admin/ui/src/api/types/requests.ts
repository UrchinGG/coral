import type { Cause, EntityRef, KeyKind, Subject } from "./refs";

export type SubjectMatch = "uuid" | "resolved_name" | "name_only" | "display_name";

export type RequestRow = {
  request_id: string;
  ts: string;
  method: string;
  route: string | null;
  path: string;
  query: string | null;
  status: number;
  outcome_status: number | null;
  cause: Cause;
  error: string | null;
  latency_ms: number;
  key_kind: KeyKind;
  key_prefix: string | null;
  requester: EntityRef;
  actor: EntityRef | null;
  ip: EntityRef | null;
  country: string | null;
  user_agent: string | null;
  subjects: Subject[];
  body_subjects: number;
  match?: SubjectMatch;
};
