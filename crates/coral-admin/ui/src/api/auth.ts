import { useQuery } from "@tanstack/react-query";
import { authGet } from "./client";
import type { MeResponse } from "./types";

export function useMe() {
  return useQuery({
    queryKey: ["me"],
    queryFn: () => authGet<MeResponse>("/auth/me"),
    retry: false,
    staleTime: 60_000,
  });
}
