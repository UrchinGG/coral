export type CoverageGap = { feature: string; from: string; to: string };

export type WriterHealth = {
  last_seen: string | null;
  lag_s: number | null;
  dropped_today: Record<string, number>;
  unmapped_today: Record<string, number>;
};

export type Coverage = {
  log_from: string | null;
  detail_from: string | null;
  detail_since: string | null;
  attribution_from: string | null;
  subjects_from: string | null;
  body_subjects_from: string | null;
  rollups_from: string | null;
  network_from: string | null;
  gaps: CoverageGap[];
  backfill: { by_name_share: Record<string, number> };
  writer: WriterHealth;
};

export type BootExtension = { id: string; title: string };

export type Boot = {
  discord_id: string;
  build: string | null;
  now: string;
  extensions: BootExtension[];
  coverage: Coverage;
};
