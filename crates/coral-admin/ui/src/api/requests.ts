import { infiniteQueryOptions, keepPreviousData, queryOptions } from "@tanstack/react-query";

import { getJson, postJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { configQuery, immutableQuery, infiniteListQuery, listQuery } from "./queryClient";
import type { Page } from "./types/refs";
import type { GroupPage, GroupQuery, RequestQuery, RouteCount } from "./types/request-explorer";
import type { RequestFull } from "./types/request-full";
import type { RequestRow } from "./types/requests";

export const REQUEST_PAGE_LIMIT = 100;

export function requestListQuery(query: RequestQuery) {
  return infiniteQueryOptions({
    staleTime: infiniteListQuery.staleTime,
    initialPageParam: infiniteListQuery.initialPageParam,
    getNextPageParam: (page: Page<RequestRow>) => page.next_cursor,
    placeholderData: keepPreviousData,
    queryKey: keys.requests.list(query),
    queryFn: ({ pageParam, signal, queryKey }) =>
      postJson<Page<RequestRow>>("/requests/query", pageParam === null ? query : { ...query, cursor: pageParam }, { signal, timingKey: queryKey }).then(withLabels),
  });
}

export function requestTail(query: RequestQuery, after: string, signal?: AbortSignal): Promise<Page<RequestRow>> {
  return postJson<Page<RequestRow>>("/requests/query", { ...query, after }, { signal }).then(withLabels);
}

export function requestGroupsQuery(query: GroupQuery) {
  return queryOptions<GroupPage>({
    ...listQuery,
    queryKey: keys.requests.groups(query),
    queryFn: ({ signal, queryKey }) => postJson<GroupPage>("/requests/groups", query, { signal, timingKey: queryKey }).then(withLabels),
  });
}

export function requestRoutesQuery() {
  return queryOptions({
    ...configQuery,
    queryKey: keys.requests.routes(),
    queryFn: ({ signal, queryKey }) => getJson<RouteCount[]>("/requests/routes", { signal, timingKey: queryKey }),
  });
}

export function requestFullQuery(requestId: string, ts: string | null) {
  const search = ts === null ? "" : `?${new URLSearchParams({ t: ts })}`;
  return queryOptions({
    ...immutableQuery,
    queryKey: keys.requests.detail(requestId, ts ?? ""),
    queryFn: ({ signal, queryKey }) => getJson<RequestFull>(`/requests/${requestId}${search}`, { signal, timingKey: queryKey }).then(withLabels),
  });
}
