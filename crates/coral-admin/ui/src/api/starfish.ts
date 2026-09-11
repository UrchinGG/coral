import { useQuery } from "@tanstack/react-query";
import { apiGet, apiPost } from "./client";
import { useAdminMutation } from "./useAdminMutation";
import type { StarfishUserDetailResponse, StarfishUserListResponse } from "./types";

export function useStarfishUsers(search: string, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (search) params.set("search", search);

  return useQuery({
    queryKey: ["starfish", "list", params.toString()],
    queryFn: () => apiGet<StarfishUserListResponse>(`/starfish?${params}`),
  });
}

export function useStarfishUser(id: number) {
  return useQuery({
    queryKey: ["starfish", "detail", id],
    queryFn: () => apiGet<StarfishUserDetailResponse>(`/starfish/${id}`),
  });
}

const starfishKeys = (id: number) => [["starfish", "detail", id], ["starfish", "list"]];

export function useSetStarfishLicenseStatus(id: number) {
  return useAdminMutation((status: string) => apiPost(`/starfish/${id}/license`, { status }), {
    successMessage: "License status updated",
    invalidateKeys: starfishKeys(id),
  });
}

export function useRevokeStarfishSessions(id: number) {
  return useAdminMutation<void, unknown>(() => apiPost(`/starfish/${id}/sessions/revoke`), {
    successMessage: "Sessions revoked",
    invalidateKeys: starfishKeys(id),
  });
}
