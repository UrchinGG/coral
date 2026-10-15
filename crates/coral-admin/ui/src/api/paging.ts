import { STALE_LIST_MS } from "./queryClient";

export function cursorPaging<T extends { next_cursor: string | null }>() {
  return {
    staleTime: STALE_LIST_MS,
    initialPageParam: null as string | null,
    getNextPageParam: (page: T) => page.next_cursor,
  };
}
