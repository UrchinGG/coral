export type MeResponse = {
  authenticated: boolean;
  discord_id: string | null;
};

export type RequestRow = {
  ts: string;
  method: string | null;
  path: string | null;
  query: string | null;
  status: number | null;
  latency_ms: number | null;
  key_prefix: string | null;
  ip: string | null;
  user_agent: string | null;
  error: string | null;
  discord_id: string | null;
  uuid: string | null;
  discord_username: string | null;
  minecraft_username: string | null;
};

export type RequestListResponse = {
  total: number;
  requests: RequestRow[];
};

export type Bucket = {
  t: string;
  total: number;
  errors: number;
};

export type PathCount = {
  path: string | null;
  count: number;
};

export type TopKey = {
  key_prefix: string | null;
  discord_id: string | null;
  uuid: string | null;
  count: number;
  errors: number;
  rate_limited: number;
  forbidden: number;
  discord_username: string | null;
  minecraft_username: string | null;
};

export type TopPath = {
  path: string | null;
  count: number;
  errors: number;
  avg_ms: number | null;
  p50_ms: number | null;
  p95_ms: number | null;
  p99_ms: number | null;
  status_2xx: number;
  status_3xx: number;
  status_4xx: number;
  status_5xx: number;
};

export type StatusClass = {
  class: number;
  count: number;
};

export type Stats = {
  hours: number;
  total: number;
  errors: number;
  avg_ms: number | null;
  status_classes: StatusClass[];
  top_keys: TopKey[];
  top_paths: TopPath[];
};

export type RateLimits = {
  available: boolean;
  capacity: number;
  used: number;
  headroom: number;
};

export type BudgetRow = {
  discord_id: string;
  discord_username: string | null;
  kind: "session" | "uuid_batch";
  used: number;
  limit: number;
  utilization: number;
};

export type MemberSummary = {
  id: number;
  discord_id: string;
  uuid: string | null;
  join_date: string;
  request_count: number;
  access_level: number;
  key_locked: boolean;
  tagging_disabled: boolean;
  has_api_key: boolean;
  strike_count: number;
  has_dev_key: boolean;
  last_seen_ip: string | null;
  is_owner: boolean;
  discord_username: string | null;
  minecraft_username: string | null;
  budget_utilization: number | null;
};

export type MemberListResponse = {
  total: number;
  members: MemberSummary[];
};

export type IpRecord = {
  ip_address: string;
  first_seen: string;
  last_seen: string;
};

export type AltAccount = {
  uuid: string;
  added_at: string;
  minecraft_username: string | null;
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

export type DevKeyView = {
  label: string;
  permissions: number;
  rate_limit: number;
  request_count: number;
  locked: boolean;
  api_key: string | null;
};

export type StarfishView = {
  license_status: string;
  has_active_session: boolean;
};

export type AuthoredTag = {
  id: number;
  uuid: string;
  kind: string;
  tag_type: string | null;
  reason: string | null;
  ts: string;
  minecraft_username: string | null;
};

export type Strike = {
  reason: string;
  struck_by: string;
  timestamp: string;
};

export type MemberDetail = {
  id: number;
  discord_id: string;
  discord_username: string | null;
  uuid: string | null;
  minecraft_username: string | null;
  api_key_preview: string | null;
  join_date: string;
  request_count: number;
  access_level: number;
  key_locked: boolean;
  tagging_disabled: boolean;
  is_owner: boolean;
  standing: StandingView;
  strikes: Strike[];
  config: unknown;
  created_at: string;
  updated_at: string;
  ips: IpRecord[];
  alt_accounts: AltAccount[];
  dev_key: DevKeyView | null;
  starfish: StarfishView | null;
  authored_tags: AuthoredTag[];
};

export type Tag = {
  id: number;
  uuid: string;
  tag_type: string;
  reason: string | null;
  added_by: string | null;
  added_by_username: string | null;
  added_on: string;
  hide_username: boolean | null;
};

export type RemovedTag = {
  add_id: number;
  uuid: string;
  tag_type: string;
  reason: string | null;
  added_by: string | null;
  added_on: string;
  removed_by: string | null;
  removed_on: string;
};

export type PlayerSummary = {
  id: number;
  uuid: string;
  minecraft_username: string | null;
  is_locked: boolean;
  lock_reason: string | null;
  locked_by: string | null;
  locked_by_username: string | null;
  locked_at: string | null;
  tags: Tag[];
};

export type PlayerListResponse = {
  total: number;
  players: PlayerSummary[];
};

export type PlayerDetailResponse = {
  player: {
    id: number;
    uuid: string;
    minecraft_username: string | null;
    is_locked: boolean;
    lock_reason: string | null;
    locked_by: string | null;
    locked_by_username: string | null;
    locked_at: string | null;
  };
  tags: Tag[];
  tag_history: RemovedTag[];
};

export type ActionRow = {
  id: number;
  actor: string;
  action: string;
  target: string;
  details: Record<string, unknown>;
  ts: string;
  actor_username: string | null;
};

export type PluginSummaryRow = {
  slug: string;
  display_name: string;
  description: string;
  author: string;
  owner_discord_id: number;
  official: boolean;
  unlisted: boolean;
  disabled: boolean;
  tags: string[];
  latest_version: string;
  updated_at: string;
  installs_30d: number;
  installs_total: number;
  rating_mean: number | null;
  rating_count: number;
  rating_bayesian: number;
  owner_discord_username: string | null;
  owner_member_id: number | null;
};

export type PluginListResponse = {
  total: number;
  plugins: PluginSummaryRow[];
};

export type Plugin = {
  id: number;
  slug: string;
  owner_user_id: number;
  repo: string;
  github_repo_id: number;
  display_name: string;
  description: string;
  tags: string[];
  homepage: string | null;
  unlisted: boolean;
  unlisted_at: string | null;
  official: boolean;
  disabled: boolean;
  disabled_reason: string | null;
  disabled_at: string | null;
  created_at: string;
  updated_at: string;
};

export type ReleaseView = {
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

export type ReviewView = {
  user_id: number;
  discord_id: string | null;
  discord_username: string | null;
  stars: number;
  review: string | null;
  updated_at: string;
};

export type PluginDetailResponse = {
  plugin: Plugin;
  owner_discord_id: string | null;
  owner_discord_username: string | null;
  owner_member_id: number | null;
  releases: ReleaseView[];
  installs_30d: number;
  installs_total: number;
  reviews: ReviewView[];
};

export type FlagKind = "budget" | "probe" | "spike" | "hypixel_headroom";

export type Flag = {
  flag_key: string;
  kind: FlagKind;
  summary: string;
  discord_id: string | null;
  discord_username: string | null;
  member_id: number | null;
};

export type PluginChangeRow = {
  slug: string;
  kind: "disabled" | "unlisted";
  reason: string | null;
  at: string;
};

export type OverviewResponse = {
  flags: Flag[];
  recent_plugin_changes: PluginChangeRow[];
};

export type PlayerSnapshotRow = {
  uuid: string;
  username: string | null;
  last_snapshot_at: string | null;
};

export type PlayerSnapshotListResponse = {
  players: PlayerSnapshotRow[];
};

export type PlayerSnapshotDetail = {
  uuid: string;
  username: string | null;
  latest: unknown;
  timestamps: string[];
};

export type GuildRow = {
  guild_id: string;
  name: string;
  tag: string | null;
  level: number;
  member_count: number;
  experience: number;
  updated_at: string;
};

export type GuildListResponse = {
  total: number;
  guilds: GuildRow[];
};

export type GuildDetail = {
  guild_id: string;
  name: string | null;
  current: unknown;
  timestamps: string[];
};

export type ResolvedIds = {
  uuids: Record<string, string>;
  discord: Record<string, string>;
};
