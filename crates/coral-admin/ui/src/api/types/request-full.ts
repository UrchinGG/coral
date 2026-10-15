import type { EntityRef, Labels, Subject } from "./refs";
import type { RequestRow } from "./requests";

export type DetailState = "present" | "not_captured" | "expired";

export type KeyState = "current" | "retired" | "unknown";

export type QueryParam = { name: string; raw: string; value: string; ref: EntityRef | null };

export type InvolvedPlayer = Subject & { current_name: string | null };

export type KeyDetail = { key: string; key_id: number | null; owner: EntityRef | null; state: KeyState };

export type CallerDetail = {
  account: EntityRef | null;
  actor: EntityRef | null;
  key: KeyDetail | null;
  starfish_user: EntityRef | null;
  plugin: EntityRef | null;
};

export type RequestNeighbors = {
  prev_same_requester: EntityRef | null;
  next_same_requester: EntityRef | null;
  same_player_5m: number;
};

export type RequestFull = {
  request: RequestRow;
  url: string;
  query_params: QueryParam[];
  headers: Record<string, string> | null;
  body: string | null;
  body_bytes: number | null;
  body_complete: boolean;
  body_json: unknown;
  server_error: string | null;
  detail_state: DetailState;
  subjects: InvolvedPlayer[];
  caller: CallerDetail;
  neighbors: RequestNeighbors;
  labels: Labels;
};
