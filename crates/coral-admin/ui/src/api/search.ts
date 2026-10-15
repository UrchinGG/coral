import { queryOptions } from "@tanstack/react-query";

import { postJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { STALE_LIST_MS } from "./queryClient";
import type { RemoteSearchRequest, SearchHit, SearchRequest, SearchResponse } from "./types/search";

export const SEARCH_LIMIT = 8;

export function searchQuery(q: string, limit = SEARCH_LIMIT) {
  return queryOptions({
    queryKey: [...keys.search.query(q), limit] as const,
    queryFn: ({ signal, queryKey }) =>
      postJson<SearchResponse>("/search", { q, limit } satisfies SearchRequest, { signal, timingKey: queryKey }).then(withLabels),
    enabled: q !== "",
    staleTime: STALE_LIST_MS,
    retry: false,
  });
}

export function remoteSearchQuery(name: string, enabled: boolean) {
  return queryOptions({
    queryKey: keys.search.remote(name),
    queryFn: ({ signal, queryKey }) =>
      postJson<SearchHit[]>("/search/remote", { name } satisfies RemoteSearchRequest, { signal, timingKey: queryKey }),
    enabled: enabled && name !== "",
    staleTime: STALE_LIST_MS,
    retry: false,
  });
}
