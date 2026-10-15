import type { EntityRef, Labels } from "./refs";

export type SearchRequest = { q: string; limit: number };

export type SearchHit = {
  ref: EntityRef;
  score: number;
  exact: boolean;
  reason: string;
  account_id: string | null;
  tab: string | null;
};

export type LookupFailure = { lookup: string; code: string; message: string };

export type SearchResponse = {
  interpretations: string[];
  hits: SearchHit[];
  remote_pending: boolean;
  failures: LookupFailure[];
  labels: Labels;
};

export type RemoteSearchRequest = { name: string };
