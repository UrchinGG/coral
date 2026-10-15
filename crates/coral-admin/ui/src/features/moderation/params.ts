import type { BlacklistFilters, BlacklistSearchField, SortDirection, StrikeFilter } from "../../api/types/platform";

export const MODERATION_TABS = ["blacklist", "strikes", "audit", "guide"] as const;
export type ModerationTab = (typeof MODERATION_TABS)[number];

export const SEARCH_FIELDS: readonly BlacklistSearchField[] = ["player", "tagger", "reason"];
export const DIRECTIONS: readonly SortDirection[] = ["desc", "asc"];
export const STRIKE_FILTERS: readonly StrikeFilter[] = ["active", "all"];

export function blacklistFiltersOf(params: URLSearchParams): BlacklistFilters {
  return {
    q: params.get("q") ?? "",
    field: oneOf(params.get("field"), SEARCH_FIELDS, "player"),
    tag: params.get("tag") ?? "",
    dir: oneOf(params.get("dir"), DIRECTIONS, "desc"),
  };
}

export function strikeFilterOf(params: URLSearchParams): StrikeFilter {
  return oneOf(params.get("state"), STRIKE_FILTERS, "active");
}

export function tabOf(params: URLSearchParams): ModerationTab {
  return oneOf(params.get("tab"), MODERATION_TABS, "blacklist");
}

function oneOf<T extends string>(raw: string | null, values: readonly T[], fallback: T): T {
  return values.find((value) => value === raw) ?? fallback;
}
