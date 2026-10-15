import type { EntityRef } from "./refs";

export type PluginChangeKind = "published" | "yanked" | "disabled" | "unlisted";

export type PluginChange = {
  at: string;
  plugin: EntityRef;
  change: PluginChangeKind;
  version: string | null;
  by: EntityRef | null;
  reason: string | null;
};

export type RecentModeration = {
  from: string;
  to: string;
  tags: { added: number; removed: number; by_type: { tag_type: string | null; count: number }[] };
  locks: { locked: number; unlocked: number; players: EntityRef[] };
  strikes: { issued: number; accounts: EntityRef[] };
  reviews: { open: number; oldest_open: string | null };
  plugins: { since: string; changes: PluginChange[] };
};
