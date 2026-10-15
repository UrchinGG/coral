import type { GuildSort } from "../../api/types/platform";

export const GUILD_SORTS: readonly GuildSort[] = ["updated", "members", "level", "experience"];

export function guildSortOf(params: URLSearchParams): GuildSort {
  return GUILD_SORTS.find((sort) => sort === params.get("sort")) ?? "updated";
}
