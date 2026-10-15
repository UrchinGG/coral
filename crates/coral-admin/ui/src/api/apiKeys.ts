import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";

import { getJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { cursorPaging } from "./paging";
import { entityQuery } from "./queryClient";
import type { KeyLookup } from "./types/apiKeys";
import type { Page } from "./types/refs";
import type { RequestRow } from "./types/requests";

export const KEY_PREFIX_LENGTH = 8;

export function keyByPrefixQuery(prefix: string) {
  return queryOptions({
    queryKey: keys.apiKeys.byPrefix(prefix),
    queryFn: ({ signal, queryKey }) => getJson<KeyLookup>(`/keys/${encodeURIComponent(prefix)}`, { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function keyByIdQuery(keyId: number) {
  return queryOptions({
    queryKey: keys.apiKeys.byId(keyId),
    queryFn: ({ signal, queryKey }) => getJson<KeyLookup>(`/keys/id/${keyId}`, { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function keyRequestsQuery(prefix: string) {
  return infiniteQueryOptions({
    queryKey: [...keys.apiKeys.all(), "requests", prefix] as const,
    queryFn: ({ signal, queryKey, pageParam }) =>
      getJson<Page<RequestRow>>(`/keys/${encodeURIComponent(prefix)}/requests${pageParam === null ? "" : `?${new URLSearchParams({ cursor: pageParam })}`}`, {
        signal,
        timingKey: queryKey,
      }).then(withLabels),
    ...cursorPaging<Page<RequestRow>>(),
  });
}
