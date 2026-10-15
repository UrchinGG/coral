import type { AccountRef, EntityRef, KeyKind, Labels, PlayerRef, Service } from "./refs";
import type { RequestRow } from "./requests";

export type PlayerForm = "undashed" | "dashed" | "name" | "former" | "path" | "body" | "display";

export type PlayerWindow = "24h" | "7d" | "14d";

export type TopWindow = "1h" | "24h";

export type PlayerName = { name: string; first_seen: string | null; last_seen: string | null; current: boolean };

export type PlayerOwner = {
  account: AccountRef;
  relation: "primary" | "alt";
  linked_at: string | null;
  unlinked_at: string | null;
  primary_shared_with: number;
};

export type TagView = {
  id: number;
  tag_type: string;
  display_name: string;
  color: string | null;
  reason: string | null;
  author: AccountRef | null;
  at: string;
  expires_at: string | null;
  hide_username: boolean;
  reviewed_by: AccountRef[];
  displaced: AccountRef | null;
};

export type LockView = { by: AccountRef | null; at: string; reason: string | null };

export type PlayerCore = {
  uuid: string;
  dashed: string;
  name: string | null;
  names: PlayerName[];
  owners: PlayerOwner[];
  blacklist: { active: TagView[]; locked: LockView | null };
  snapshots: { count: number; first: string | null; last: string | null };
  session_markers: number;
  requests: { last_24h: number; last_14d: number; requesters_14d: number };
  labels: Labels;
};

export type PlayerRequester = {
  key: string;
  requester: EntityRef;
  actor: EntityRef | null;
  key_kind: KeyKind;
  service: Service;
  key_prefix: string | null;
  requests: number;
  rate_limited: number;
  errors: number;
  first: string;
  last: string;
  forms: Partial<Record<PlayerForm, number>>;
};

export type PlayerRequesters = {
  from: string;
  to: string;
  clamped_from: string | null;
  groups: PlayerRequester[];
  forms: Partial<Record<PlayerForm, number>>;
  uncaptured_batches: number | null;
  labels: Labels;
};

export type PlayerRequestRow = RequestRow & { matched_form: PlayerForm };

export type TagEnding = { how: "cleared" | "replaced" | "expired"; by: AccountRef | null; at: string };

export type TagRecord = TagView & { ended: TagEnding | null };

export type LockEvent = { locked: boolean; by: AccountRef | null; at: string; reason: string | null; expires_at: string | null };

export type TagType = { name: string; display_name: string; color: string };

export type PlayerTags = {
  active: TagView[];
  history: TagRecord[];
  locks: LockEvent[];
  locked: LockView | null;
  tag_types: TagType[];
  labels: Labels;
};

export type NewTag = { tag_type: string; reason: string; hide_username: boolean };

export type SnapshotDay = { day: string; count: number };

export type SnapshotRow = { ts: string; source: string | null; baseline: boolean; username: string | null };

export type SnapshotPage = { days: SnapshotDay[]; rows: SnapshotRow[]; next_cursor: string | null };

export type SnapshotAt = { ts: string; data: unknown };

export type TopPlayers = {
  window: TopWindow;
  computed_at: string;
  players: { player: string; requests: number; last_seen: string }[];
  labels: Labels;
};

export type TaggedPlayer = {
  id: number;
  player: PlayerRef;
  tag_type: string;
  display_name: string;
  color: string | null;
  author: AccountRef | null;
  at: string;
};

export type RecentlyTagged = { items: TaggedPlayer[]; labels: Labels };
