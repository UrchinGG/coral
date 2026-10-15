import type { AccountRef, Cause, EntityRef, IpRef, Labels, PluginRef, RequestRef, SubjectForm } from "./refs";

export type CursorPage<T> = { items: T[]; next_cursor: string | null; labels: Labels };

export type AccountSort = "member" | "joined" | "requests" | "access";
export type SortDirection = "asc" | "desc";

export type AccountListFilters = {
  rank: number | null;
  locked: boolean | null;
  has_key: boolean | null;
  has_dev_key: boolean | null;
  starfish: boolean | null;
  sort: AccountSort;
  dir: SortDirection;
};

export type AccountRow = {
  account: AccountRef;
  minecraft: EntityRef | null;
  alts: number;
  access_level: number;
  key_locked: boolean;
  tagging_disabled: boolean;
  has_key: boolean;
  has_dev_key: boolean;
  starfish_license: string | null;
  active_strikes: number;
  request_count: number;
  requests_24h: number;
  rate_limited_24h: number;
  last_active: string | null;
  joined: string;
};

export type Totals = { requests: number; rate_limited_caller: number; rate_limited_upstream: number; errors: number };

export type DevKey = {
  key_id: number | null;
  key: string;
  label: string;
  permissions: number;
  rate_limit: number;
  request_count: number;
  locked: boolean;
  created_at: string;
};

export type StrikeStatus = "active" | "expired" | "revoked";

export type StrikeRow = {
  id: number;
  reason: string;
  struck_by: EntityRef;
  issued_at: string;
  expires_at: string | null;
  revoked_at: string | null;
  revoked_by: EntityRef | null;
  status: StrikeStatus;
};

export type StandingView = {
  can_vote: boolean;
  vote_reason: string;
  can_tag: boolean;
  tag_reason: string;
  effective_level: number;
  strike_count: number;
  accepted_tags: number;
  rejected_tags: number;
  accurate_verdicts: number;
  incorrect_verdicts: number;
  bonus_verdicts: number;
};

export type MemberView = {
  id: number;
  uuid: string | null;
  access_level: number;
  key_locked: boolean;
  tagging_disabled: boolean;
  join_date: string;
  created_at: string;
  last_request_at: string | null;
  request_count: number;
  config: unknown;
  is_owner: boolean;
  row: Record<string, unknown>;
};

export type MinecraftLink = { player: EntityRef; relation: LinkRelation; linked_at: string | null; also_primary_of: AccountRef[] };

export type LinkRelation = "primary" | "alt";

export type AccountCore = {
  account: AccountRef;
  member: MemberView | null;
  keys: { personal: string | null; personal_id: number | null; developer: DevKey | null; history_count: number };
  minecraft: MinecraftLink[];
  former_minecraft: number;
  standing: StandingView | null;
  strikes: { active: number; items: StrikeRow[] };
  starfish: { user_id: number; license_status: string; github_username: string | null; active_session: boolean } | null;
  activity: { h24: Totals; d14: Totals; mentions_24h: number; last_ip: IpRef | null; ip_count: number };
  counts: { ips: number; agents: number; links: number; authored_tags: number; reviewed_tags: number; reviews: number; votes: number; admin_actions: number };
  labels: Labels;
};

export type ActivityWindow = "24h" | "7d" | "30d" | "400d";

export type ActivityPoint = { t: string; requests: number; rate_limited_caller: number; rate_limited_upstream: number; errors: number };

export type BreakdownRow = { route: string | null; method: string; status: number; cause: Cause; requests: number };

export type AccountActivity = {
  from: string;
  to: string;
  step_s: number;
  totals: Totals;
  mentions: number;
  series: ActivityPoint[];
  breakdown: BreakdownRow[];
};

export type IpUse = {
  ip: IpRef;
  requests: number;
  first_seen: string;
  last_seen: string;
  imported: boolean;
  shared_with: AccountRef[];
  shared_count: number;
};

export type AgentUse = { user_agent: string; requests: number; first_seen: string; last_seen: string };

export type AccountNetwork = {
  ips: IpUse[];
  agents: AgentUse[];
  countries: { country: string | null; requests: number }[];
  linked_accounts: { account: AccountRef; ips: IpRef[] }[];
  labels: Labels;
};

export type LookupWindow = "24h" | "14d";

export type LookupChannel = { requests: number; player_mentions: number };

export type LookupSummary = { from: string; to: string; via_key: LookupChannel; via_bot: LookupChannel; uncaptured_batches: number };

export type LookupVia = "key" | "bot";

export type TopLookups = {
  sample_size: number;
  sample_from: string | null;
  window_from: string;
  exact: boolean;
  players: { player: EntityRef; requests: number; last: string; via: LookupVia | "both" }[];
  labels: Labels;
};

export type LookupRow = { ts: string; request: RequestRef; player: string | null; name: string | null; form: SubjectForm; via: LookupVia };

export type TimelineKind =
  | "admin_action"
  | "strike_received"
  | "strike_revoked"
  | "strike_issued"
  | "player_event"
  | "tag_authored"
  | "tag_reviewed"
  | "review_submitted"
  | "vote"
  | "key_created"
  | "key_retired"
  | "linked"
  | "unlinked";

export type TimelineEvent = {
  key: string;
  at: string;
  kind: TimelineKind;
  actor: AccountRef | null;
  target: AccountRef | null;
  target_text: string | null;
  player: EntityRef | null;
  action: string | null;
  detail: string | null;
  data: Record<string, unknown> | null;
};

export type KeyRecord = { key_id: number; kind: string; key: string; created_at: string | null; retired_at: string | null };

export type LinkRecord = { uuid: string; relation: LinkRelation; linked_at: string | null; unlinked_at: string | null };

export type Limiter = "personal" | "developer" | "session" | "session_uuids" | "key_uuids" | "tagging";

export type Budget = { limiter: Limiter; used: number; limit: number | null; window_s: number };

export type StarfishHwid = {
  id: number;
  hwid_hash: string;
  is_active: boolean;
  registered_at: string;
  machine_guid_hash: string | null;
  smbios_uuid_hash: string | null;
  disk_serial_hash: string | null;
  cpu_id_hash: string | null;
  baseboard_serial_hash: string | null;
  components_at: string | null;
};

export type StarfishAccount = {
  user: { id: number; license_status: string; github_username: string | null; github_linked_at: string | null; created_at: string; updated_at: string };
  hwids: StarfishHwid[];
  sessions: { id: number; hwid_id: number; issued_at: string; expires_at: string; last_heartbeat_at: string; active: boolean }[];
  refresh_tokens: { id: number; hwid_id: number; created_at: string; last_used_at: string }[];
  installed_plugins: { plugin: PluginRef; installed_version: string; latest_version: string; disabled: boolean }[];
  owned_plugins: { plugin: PluginRef; official: boolean; unlisted: boolean; disabled: boolean; latest_version: string | null }[];
};

export type StrikeInput = { reason: string; duration_days: number | null };

export type DevKeyInput = { label: string; permissions: number; rate_limit: number };

export type ResetLimits = { cleared: Limiter[] };
