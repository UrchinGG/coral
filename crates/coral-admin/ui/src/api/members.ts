import { useQuery } from "@tanstack/react-query";
import { apiDelete, apiGet, apiPost } from "./client";
import { useAdminMutation } from "./useAdminMutation";
import type { DevKeyView, MemberDetail, MemberListResponse } from "./types";

export type MemberListFilters = {
  search: string;
  sort: string;
  dir: string;
  rank: string;
  locked: boolean;
  haskey: boolean;
};

export function useMembers(filters: MemberListFilters, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (filters.search) params.set("search", filters.search);
  if (filters.sort) params.set("sort", filters.sort);
  if (filters.dir) params.set("dir", filters.dir);
  if (filters.rank) params.set("rank", filters.rank);
  if (filters.locked) params.set("locked", "true");
  if (filters.haskey) params.set("haskey", "true");

  return useQuery({
    queryKey: ["members", "list", params.toString()],
    queryFn: () => apiGet<MemberListResponse>(`/members?${params}`),
  });
}

export function useMember(id: number) {
  return useQuery({
    queryKey: ["members", "detail", id],
    queryFn: () => apiGet<MemberDetail>(`/members/${id}`),
  });
}

const memberKeys = (id: number) => [["members", "detail", id], ["members", "list"]];

export function useLockMember(id: number) {
  return useAdminMutation<void, unknown>(() => apiPost(`/members/${id}/lock`), {
    successMessage: "Account locked",
    invalidateKeys: memberKeys(id),
  });
}

export function useUnlockMember(id: number) {
  return useAdminMutation<void, unknown>(() => apiPost(`/members/${id}/unlock`), {
    successMessage: "Account unlocked",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetAccessLevel(id: number) {
  return useAdminMutation((level: number) => apiPost(`/members/${id}/access-level`, { level }), {
    successMessage: "Access level updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetTaggingDisabled(id: number) {
  return useAdminMutation((disabled: boolean) => apiPost(`/members/${id}/tagging-disabled`, { disabled }), {
    successMessage: "Tagging permission updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useAddStrike(id: number) {
  return useAdminMutation((reason: string) => apiPost(`/members/${id}/strikes`, { reason }), {
    successMessage: "Strike added",
    invalidateKeys: memberKeys(id),
  });
}

export function useRemoveStrike(id: number) {
  return useAdminMutation((index: number) => apiDelete(`/members/${id}/strikes/${index}`), {
    successMessage: "Strike removed",
    invalidateKeys: memberKeys(id),
  });
}

export function useRegenerateApiKey(id: number) {
  return useAdminMutation<void, { api_key: string }>(
    () => apiPost<{ api_key: string }>(`/members/${id}/api-key/regenerate`),
    { successMessage: "API key regenerated", invalidateKeys: memberKeys(id) },
  );
}

export function useResetRateLimit(id: number) {
  return useAdminMutation<void, unknown>(() => apiPost(`/members/${id}/ratelimit/reset`), {
    successMessage: "Rate-limit budget reset",
    invalidateKeys: memberKeys(id),
  });
}

export function useCreateDevKey(id: number) {
  return useAdminMutation(
    (req: { label: string; permissions: number; rate_limit: number }) =>
      apiPost<DevKeyView>(`/members/${id}/dev-key`, req),
    { successMessage: "Developer key created", invalidateKeys: memberKeys(id) },
  );
}

export function useDeleteDevKey(id: number) {
  return useAdminMutation<void, unknown>(() => apiDelete(`/members/${id}/dev-key`), {
    successMessage: "Developer key deleted",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetDevKeyLocked(id: number) {
  return useAdminMutation((locked: boolean) => apiPost(`/members/${id}/dev-key/lock`, { locked }), {
    successMessage: "Developer key updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetDevKeyRateLimit(id: number) {
  return useAdminMutation((rate_limit: number) => apiPost(`/members/${id}/dev-key/rate-limit`, { rate_limit }), {
    successMessage: "Developer key rate limit updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetDevKeyPermissions(id: number) {
  return useAdminMutation((permissions: number) => apiPost(`/members/${id}/dev-key/permissions`, { permissions }), {
    successMessage: "Developer key permissions updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useSetLicenseStatus(id: number) {
  return useAdminMutation((status: string) => apiPost(`/members/${id}/starfish/license`, { status }), {
    successMessage: "License status updated",
    invalidateKeys: memberKeys(id),
  });
}

export function useRevokeStarfishSessions(id: number) {
  return useAdminMutation<void, unknown>(() => apiPost(`/members/${id}/starfish/sessions/revoke`), {
    successMessage: "Starfish sessions revoked",
    invalidateKeys: memberKeys(id),
  });
}
