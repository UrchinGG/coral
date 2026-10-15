import type { ActivityWindow, Limiter, LookupWindow, StrikeStatus } from "../../api/types/accounts";
import type { Tone } from "../../kit/tone";

export const ACCOUNT_TABS = ["activity", "network", "lookups", "moderation", "starfish", "raw"] as const;
export type AccountTab = (typeof ACCOUNT_TABS)[number];
export const DEFAULT_TAB: AccountTab = "activity";

export const ACTIVITY_WINDOWS: readonly ActivityWindow[] = ["24h", "7d", "30d", "400d"];
export const DEFAULT_ACTIVITY_WINDOW: ActivityWindow = "24h";
export const LOOKUP_WINDOWS: readonly LookupWindow[] = ["24h", "14d"];
export const DEFAULT_LOOKUP_WINDOW: LookupWindow = "14d";

export const ACCESS_LEVELS = [
  { level: 0, label: "Default" },
  { level: 2, label: "Helper" },
  { level: 3, label: "Moderator" },
  { level: 4, label: "Admin" },
  { level: 5, label: "Owner" },
] as const;

export const DEV_KEY_PERMISSIONS = [
  { bit: 1, label: "Player data" },
  { bit: 2, label: "Hypixel" },
  { bit: 4, label: "All sessions" },
] as const;

export const LIMITER_LABEL: Record<Limiter, { label: string; unit: string }> = {
  personal: { label: "Personal key", unit: "per 5 min" },
  developer: { label: "Developer key", unit: "per 5 min" },
  session: { label: "Starfish sessions", unit: "per 5 min" },
  session_uuids: { label: "Starfish players", unit: "distinct / 5 min" },
  key_uuids: { label: "Key players", unit: "distinct / 5 min" },
  tagging: { label: "Tagging", unit: "per 5 min" },
};

export const STRIKE_TONE: Record<StrikeStatus, Tone> = { active: "danger", expired: "neutral", revoked: "neutral" };

export const LICENSE_TONE: Record<string, Tone> = { active: "ok", suspended: "danger", inactive: "neutral" };

export const STRIKE_DURATIONS = [
  { value: "14", label: "14 days" },
  { value: "30", label: "30 days" },
  { value: "none", label: "No expiry" },
] as const;

export const STRIKE_REASON_MAX_CHARS = 100;
export const DEV_KEY_LABEL_MAX_CHARS = 64;
export const DEV_KEY_MAX_RATE_LIMIT = 100_000;
export const DEV_KEY_DEFAULT_RATE_LIMIT = 600;

export function accessLabel(level: number): string {
  return ACCESS_LEVELS.findLast((rank) => rank.level <= level)?.label ?? ACCESS_LEVELS[0].label;
}

export function permissionLabels(bits: number): string {
  const granted = DEV_KEY_PERMISSIONS.filter((permission) => (bits & permission.bit) !== 0).map((permission) => permission.label);
  return granted.length === 0 ? "none" : granted.join(", ");
}
