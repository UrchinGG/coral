import { keepPreviousData, QueryClient, type QueryFunctionContext } from "@tanstack/react-query";

import { getJson, isApiError, postJson } from "./http";
import { forgetTiming } from "./timing";

export const STALE_LIVE_MS = 5_000;
export const STALE_LIST_MS = 15_000;
export const STALE_ENTITY_MS = 60_000;
export const STALE_CONFIG_MS = 300_000;
export const LABEL_STALE_MS = 3_600_000;
export const GC_MS = 600_000;
export const POLL_PULSE_MS = 15_000;
export const POLL_FLAGS_MS = 30_000;
export const POLL_LIMITS_MS = 10_000;
export const POLL_TAIL_MS = 5_000;
export const RETRY_DELAYS_MS = [300, 900] as const;
export const SCAN_AUTO_CONTINUE_HOURS = 24;

export const entityQuery = { staleTime: STALE_ENTITY_MS } as const;

export const immutableQuery = { staleTime: Infinity } as const;

export const listQuery = { staleTime: STALE_LIST_MS, placeholderData: keepPreviousData };

export const configQuery = { staleTime: STALE_CONFIG_MS } as const;

export const infiniteListQuery = {
  staleTime: STALE_LIST_MS,
  initialPageParam: null as string | null,
  getNextPageParam: (page: { next_cursor: string | null }) => page.next_cursor,
};

export function liveQuery(intervalMs: number) {
  return { staleTime: STALE_LIVE_MS, refetchInterval: intervalMs, refetchIntervalInBackground: false } as const;
}

export function getQuery<T>(path: string) {
  return ({ signal, queryKey }: QueryFunctionContext) => getJson<T>(path, { signal, timingKey: queryKey });
}

export function postQuery<T>(path: string, body: unknown) {
  return ({ signal, queryKey }: QueryFunctionContext) => postJson<T>(path, body, { signal, timingKey: queryKey });
}

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      gcTime: GC_MS,
      retry: shouldRetry,
      retryDelay: (attempt) => RETRY_DELAYS_MS[Math.min(attempt, RETRY_DELAYS_MS.length - 1)],
    },
    mutations: {
      retry: false,
    },
  },
});

queryClient.getQueryCache().subscribe((event) => {
  if (event.type === "removed") forgetTiming(event.query.queryHash);
});

function shouldRetry(failureCount: number, error: Error): boolean {
  return failureCount < RETRY_DELAYS_MS.length && isApiError(error) && error.retryable;
}
