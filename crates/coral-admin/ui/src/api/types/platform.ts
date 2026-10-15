import type { AccountRef, EntityRef, GuildRef, Labels, PlayerRef, PluginRef } from "./refs";

export type RowPage<T> = { items: T[]; next_cursor: string | null; labels: Labels };

export type TagType = { name: string; display_name: string; color: string };

export type BlacklistSearchField = "player" | "tagger" | "reason";

export type SortDirection = "desc" | "asc";

export type BlacklistFilters = { q: string; field: BlacklistSearchField; tag: string; dir: SortDirection };

export type PlayerLock = { by: AccountRef | null; at: string; reason: string | null };

export type BlacklistRow = {
  id: number;
  player: PlayerRef;
  tag_type: string;
  reason: string | null;
  author: AccountRef | null;
  at: string;
  expires_at: string | null;
  hide_username: boolean;
  lock: PlayerLock | null;
};

export type StrikeFilter = "active" | "all";

export type StrikeState = "active" | "expired" | "revoked";

export type StrikeRow = {
  id: number;
  account: AccountRef;
  reason: string;
  struck_by: AccountRef;
  issued_at: string;
  expires_at: string | null;
  revoked_at: string | null;
  revoked_by: AccountRef | null;
  source_ref: string | null;
  state: StrikeState;
};

export type AuditRow = {
  id: number;
  ts: string;
  actor: AccountRef;
  action: string;
  target: string;
  target_ref: EntityRef | null;
  details: unknown;
};

export type GuideContent = { body: string };

export type GuideStatus = {
  posted: boolean;
  forum_channel_id: string | null;
  thread_id: string | null;
  posted_at: string | null;
  posted_by: AccountRef | null;
  content_updated_at: string;
  up_to_date: boolean;
};

export type PingRoles = { review_role_id: string | null; dispute_role_id: string | null; confirm_role_id: string | null };

export type Guide = { content: GuideContent; status: GuideStatus; ping_roles: PingRoles; labels: Labels };

export type ReviewRules = {
  accept_threshold: number;
  reject_threshold: number;
  vote_grant_approvals: number;
  tag_grant_correct: number;
  earn_ratio: number;
  bonus_weight: number;
  revoke_strikes: number;
};

export type RulesView = { rules: ReviewRules; defaults: ReviewRules };

export type RulesUpdate = { rules: ReviewRules; vote_changes: number; tag_changes: number };

export type ServerConfigValues = {
  link_role_id: string | null;
  unlinked_role_id: string | null;
  link_channel_id: string | null;
  link_message_id: string | null;
  nickname_template: string | null;
};

export type AutoroleRule = { id: number; role_id: string; condition: string };

export type ServerConfig = {
  guild_id: string;
  config: ServerConfigValues;
  rules: AutoroleRule[];
  preview_context: Record<string, unknown> | null;
};

export type DiscordRole = { id: string; name: string; color: string | null; position: number; managed: boolean; assignable: boolean; bot_assignable: boolean | null };

export type DiscordChannel = { id: string; name: string; category: string | null; kind: "text" | "forum" };

export type Directory = {
  as_of: string;
  guild: { id: string; name: string; icon_url: string | null };
  roles: DiscordRole[];
  channels: DiscordChannel[];
};

export type Population = { as_of: string; member_count: number; linked_members: number; role_members: Record<string, number> };

export type SyncJobState = "queued" | "running" | "done" | "cancelled" | "failed";

export type SyncJob = {
  id: number;
  kind: string;
  label: string;
  state: SyncJobState;
  processed: number;
  total: number;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};

export type SyncJobs = { jobs: SyncJob[] };

export type StartedJob = { job: SyncJob | null };

export type SyncJobEvent =
  | { type: "snapshot"; job: SyncJob }
  | { type: "progress"; processed: number; total: number }
  | { type: "finished"; status: SyncJobState }
  | { type: "failed"; message: string };

export type StarfishUser = {
  user_id: number;
  account: AccountRef;
  license_status: string;
  github_username: string | null;
  hwids: number;
  active_session: boolean;
  last_heartbeat_at: string | null;
  plugins_installed: number;
  plugins_owned: number;
  updated_at: string;
};

export type PluginRow = {
  plugin: PluginRef;
  description: string;
  owner: AccountRef | null;
  owner_github: string | null;
  official: boolean;
  unlisted: boolean;
  disabled: boolean;
  latest_version: string | null;
  installs_30d: number;
  installs_total: number;
  rating_mean: number | null;
  rating_count: number;
  created_at: string;
  updated_at: string;
};

export type PluginInfo = {
  slug: string;
  display_name: string;
  description: string;
  repo: string;
  homepage: string | null;
  official: boolean;
  unlisted: boolean;
  unlisted_at: string | null;
  disabled: boolean;
  disabled_reason: string | null;
  disabled_at: string | null;
  created_at: string;
  updated_at: string;
};

export type PluginRelease = {
  id: number;
  version: string;
  git_sha: string;
  asset_url: string;
  asset_sha256: string;
  content_sha256: string | null;
  asset_size: number;
  changelog: string | null;
  yanked: boolean;
  yanked_at: string | null;
  yanked_reason: string | null;
  created_at: string;
};

export type PluginReview = { user_id: number; reviewer: AccountRef | null; stars: number; review: string | null; updated_at: string };

export type PluginDetail = {
  plugin: PluginInfo;
  owner: AccountRef | null;
  installs_30d: number;
  installs_total: number;
  releases: PluginRelease[];
  reviews: PluginReview[];
  labels: Labels;
};

export type GuildSort = "updated" | "members" | "level" | "experience";

export type GuildRow = { guild: GuildRef; tag: string | null; level: number; member_count: number; experience: number; updated_at: string };

export type GuildDetail = { guild: GuildRef; summary: GuildRow | null; current: unknown; snapshots: string[] };

export type GuildSnapshot = { at: string; snapshot: unknown };

export type ResolveKind = "discord" | "player" | "name" | "ip" | "unknown";

export type Resolution = { input: string; kind: ResolveKind; entity: EntityRef | null; note: string };

export type Resolved = { results: Resolution[]; labels: Labels };

export type JobState = "running" | "waiting" | "succeeded" | "failed" | "interrupted";

export type JobStatus = {
  name: string;
  instance: string;
  state: JobState;
  started_at: string;
  updated_at: string;
  finished_at: string | null;
  progress: unknown;
  error: string | null;
};

export type PassOutcome = "done" | "behind" | "idle" | "locked" | "waiting" | "failed";

export type PassReport = {
  at: string;
  duration_ms: number;
  outcome: PassOutcome;
  from: string | null;
  to: string | null;
  written: number;
  removed: number;
  error: string | null;
};

export type MaintenanceJob = { name: string; status: JobStatus | null };

export type BackfillSpan = { from: string; to: string; kind: "before_coverage" | "gap" };

export type Maintenance = {
  jobs: MaintenanceJob[];
  rollups: { log_from: string | null; watermarks: { name: string; at: string }[]; dirty_ranges: number };
  pending: { subjects: { spans: BackfillSpan[] }; discord_names: number; indexes: string[] };
  window_open: boolean;
};
