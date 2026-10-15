import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";

import { getJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { cursorPaging } from "./paging";
import { entityQuery } from "./queryClient";
import type { IpCore } from "./types/ips";
import type { Page } from "./types/refs";
import type { RequestRow } from "./types/requests";

export function ipCoreQuery(ip: string) {
  return queryOptions({
    queryKey: keys.ips.core(ip),
    queryFn: ({ signal, queryKey }) => getJson<IpCore>(ipPath(ip), { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function ipRequestsQuery(ip: string) {
  return infiniteQueryOptions({
    queryKey: [...keys.ips.all(), "requests", ip] as const,
    queryFn: ({ signal, queryKey, pageParam }) =>
      getJson<Page<RequestRow>>(`${ipPath(ip)}/requests${pageParam === null ? "" : `?${new URLSearchParams({ cursor: pageParam })}`}`, { signal, timingKey: queryKey }).then(withLabels),
    ...cursorPaging<Page<RequestRow>>(),
  });
}

function ipPath(ip: string): string {
  return `/ips/${encodeURIComponent(ip)}`;
}
