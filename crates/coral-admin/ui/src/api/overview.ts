import { useQuery } from "@tanstack/react-query";
import { apiGet, apiPost } from "./client";
import { useAdminMutation } from "./useAdminMutation";
import type { OverviewResponse } from "./types";

export function useOverview() {
  return useQuery({
    queryKey: ["overview"],
    queryFn: () => apiGet<OverviewResponse>("/overview"),
    refetchInterval: 30_000,
  });
}

export function useDismissFlag() {
  return useAdminMutation<string, unknown>((flagKey: string) => apiPost("/overview/dismiss", { flag_key: flagKey }), {
    successMessage: "Flag dismissed for 24h",
    invalidateKeys: [["overview"]],
  });
}
