import { useQuery } from "@tanstack/react-query";
import { apiGet } from "./client";
import type { ActionRow } from "./types";

export function useRecentActions(limit: number) {
  return useQuery({
    queryKey: ["actions", "recent", limit],
    queryFn: () => apiGet<ActionRow[]>(`/actions?limit=${limit}`),
    refetchInterval: 15_000,
  });
}
