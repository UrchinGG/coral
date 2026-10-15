import { queryOptions } from "@tanstack/react-query";

import { keys } from "./keys";
import { getQuery } from "./queryClient";
import type { Boot } from "./types/session";

export const SESSION_POLL_MS = 60_000;

export function sessionQuery(boot: Boot) {
  return queryOptions({
    queryKey: keys.session(),
    queryFn: getQuery<Boot>("/session"),
    initialData: boot,
    staleTime: SESSION_POLL_MS,
    refetchInterval: SESSION_POLL_MS,
  });
}
