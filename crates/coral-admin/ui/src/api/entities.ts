import type { QueryKey } from "@tanstack/react-query";

import { keys } from "./keys";
import { entityQuery, getQuery, immutableQuery, queryClient } from "./queryClient";
import type { EntityRef } from "./types/refs";

type CoreQuery = {
  queryKey: QueryKey;
  queryFn: ReturnType<typeof getQuery<unknown>>;
  staleTime: number;
};

export function entityCoreQuery(ref: EntityRef): CoreQuery | null {
  switch (ref.kind) {
    case "account":
      return core(keys.accounts.core(ref.discord_id), `/accounts/${ref.discord_id}`);
    case "player":
      return core(keys.players.core(ref.uuid), `/players/${ref.uuid}`);
    case "request":
      return {
        queryKey: keys.requests.detail(ref.request_id, ref.ts),
        queryFn: getQuery<unknown>(`/requests/${ref.request_id}?${new URLSearchParams({ t: ref.ts })}`),
        ...immutableQuery,
      };
    case "ip":
      return core(keys.ips.core(ref.ip), `/ips/${encodeURIComponent(ref.ip)}`);
    case "key":
      return ref.key_id === null
        ? core(keys.apiKeys.byPrefix(ref.prefix), `/keys/${ref.prefix}`)
        : core(keys.apiKeys.byId(ref.key_id), `/keys/id/${ref.key_id}`);
    case "guild":
      return core(keys.guilds.core(ref.guild_id), `/guilds/${ref.guild_id}`);
    case "plugin":
      return core(keys.plugins.core(ref.slug), `/plugins/${encodeURIComponent(ref.slug)}`);
    case "name":
    case "internal":
    case "anonymous":
      return null;
  }
}

export function prefetchEntity(ref: EntityRef): void {
  const query = entityCoreQuery(ref);
  if (query !== null) void queryClient.prefetchQuery(query);
}

function core(queryKey: QueryKey, path: string): CoreQuery {
  return { queryKey, queryFn: getQuery<unknown>(path), ...entityQuery };
}
