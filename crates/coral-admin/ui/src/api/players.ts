import { useQuery } from "@tanstack/react-query";
import { apiDelete, apiGet, apiPost } from "./client";
import { useAdminMutation } from "./useAdminMutation";
import type { PlayerDetailResponse, PlayerListResponse } from "./types";

export type PlayerListFilters = {
  search: string;
  field: string;
  tag_type: string;
  dir: string;
};

export function usePlayers(filters: PlayerListFilters, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (filters.search) params.set("search", filters.search);
  if (filters.field) params.set("field", filters.field);
  if (filters.tag_type) params.set("tag_type", filters.tag_type);
  if (filters.dir) params.set("dir", filters.dir);

  return useQuery({
    queryKey: ["players", "list", params.toString()],
    queryFn: () => apiGet<PlayerListResponse>(`/blacklist?${params}`),
  });
}

export function usePlayer(uuid: string) {
  return useQuery({
    queryKey: ["players", "detail", uuid],
    queryFn: () => apiGet<PlayerDetailResponse | null>(`/blacklist/${uuid}`),
    enabled: !!uuid,
  });
}

const playerKeys = (uuid: string) => [["players", "detail", uuid], ["players", "list"]];

export function useAddTag(uuid: string) {
  return useAdminMutation(
    (req: { tag_type: string; reason: string; hide_username: boolean }) => apiPost(`/blacklist/${uuid}/tags`, req),
    { successMessage: "Tag added", invalidateKeys: playerKeys(uuid) },
  );
}

export function useRemoveTag(uuid: string) {
  return useAdminMutation((tagType: string) => apiDelete(`/blacklist/${uuid}/tags/${tagType}`), {
    successMessage: "Tag removed",
    invalidateKeys: playerKeys(uuid),
  });
}

export function useLockPlayer(uuid: string) {
  return useAdminMutation((reason: string) => apiPost(`/blacklist/${uuid}/lock`, { reason: reason || null }), {
    successMessage: "Player locked",
    invalidateKeys: playerKeys(uuid),
  });
}

export function useUnlockPlayer(uuid: string) {
  return useAdminMutation<void, unknown>(() => apiPost(`/blacklist/${uuid}/unlock`), {
    successMessage: "Player unlocked",
    invalidateKeys: playerKeys(uuid),
  });
}
