import { queryOptions, useQuery } from "@tanstack/react-query";

import { getQuery, liveQuery, POLL_FLAGS_MS } from "./queryClient";
import type { AdminActionPage } from "./types/actions";

export const RECENT_ACTIONS = 20;

const actionKeys = {
  recent: (limit: number) => ["actions", "recent", limit] as const,
};

export function recentActionsQuery(limit = RECENT_ACTIONS) {
  return queryOptions({
    queryKey: actionKeys.recent(limit),
    queryFn: getQuery<AdminActionPage>(`/actions?${new URLSearchParams({ limit: String(limit) })}`),
    ...liveQuery(POLL_FLAGS_MS),
  });
}

export function useRecentActions(limit: number) {
  return useQuery(recentActionsQuery(limit));
}
