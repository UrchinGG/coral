import type { AccountRef, Labels } from "./refs";

export type KeyState = "current" | "retired";

export type KeyMatch = {
  key_id: number;
  key?: string;
  prefix: string;
  account: AccountRef;
  kind: string;
  state: KeyState;
  locked: boolean;
  created_at: string | null;
  retired_at: string | null;
};

export type KeyLookup = {
  prefix: string;
  internal: boolean;
  matches: KeyMatch[];
  unattributed: { requests_14d: number; last: string | null; capped: boolean };
  labels: Labels;
};
