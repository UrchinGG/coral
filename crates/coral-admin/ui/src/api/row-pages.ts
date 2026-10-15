import { infiniteQueryOptions, keepPreviousData } from "@tanstack/react-query";

import { getJson } from "./http";
import { withLabels } from "./labels";
import { STALE_LIST_MS } from "./queryClient";
import type { RowPage } from "./types/platform";

export function rowPages<T>(queryKey: readonly unknown[], path: string, params: URLSearchParams) {
  return infiniteQueryOptions({
    queryKey,
    queryFn: ({ pageParam, signal, queryKey: key }) => {
      const page = new URLSearchParams(params);
      if (pageParam !== null) page.set("cursor", pageParam);
      const query = page.toString();
      return getJson<RowPage<T>>(query === "" ? path : `${path}?${query}`, { signal, timingKey: key }).then(withLabels);
    },
    initialPageParam: null as string | null,
    getNextPageParam: (page: RowPage<T>) => page.next_cursor,
    staleTime: STALE_LIST_MS,
    placeholderData: keepPreviousData,
  });
}

export function rowsOf<T>(pages: readonly RowPage<T>[] | undefined): T[] {
  return pages?.flatMap((page) => page.items) ?? [];
}
