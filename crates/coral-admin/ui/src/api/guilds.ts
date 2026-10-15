import { queryOptions } from "@tanstack/react-query";

import { keys } from "./keys";
import { entityQuery, getQuery, immutableQuery } from "./queryClient";
import { rowPages } from "./row-pages";
import type { GuildDetail, GuildRow, GuildSnapshot, GuildSort } from "./types/platform";

export function guildsQuery(q: string, sort: GuildSort) {
  const params = new URLSearchParams({ sort });
  if (q !== "") params.set("q", q);
  return rowPages<GuildRow>(keys.guilds.list(q, sort), "/guilds", params);
}

export function guildQuery(guildId: string) {
  return queryOptions({ queryKey: keys.guilds.core(guildId), queryFn: getQuery<GuildDetail>(`/guilds/${guildId}`), ...entityQuery });
}

export function guildSnapshotQuery(guildId: string, ts: string) {
  return queryOptions({
    queryKey: keys.guilds.at(guildId, ts),
    queryFn: getQuery<GuildSnapshot>(`/guilds/${guildId}/at?${new URLSearchParams({ ts })}`),
    ...immutableQuery,
  });
}
