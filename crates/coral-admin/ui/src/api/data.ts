import { useQuery } from "@tanstack/react-query";
import { apiGet } from "./client";
import type {
  GuildDetail,
  GuildListResponse,
  PlayerSnapshotDetail,
  PlayerSnapshotListResponse,
  ResolvedIds,
} from "./types";

export function usePlayerSnapshots(search: string, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (search) params.set("search", search);

  return useQuery({
    queryKey: ["data", "players", params.toString()],
    queryFn: () => apiGet<PlayerSnapshotListResponse>(`/players?${params}`),
  });
}

export function usePlayerSnapshotDetail(uuid: string | null) {
  return useQuery({
    queryKey: ["data", "players", "detail", uuid],
    queryFn: () => apiGet<PlayerSnapshotDetail>(`/players/${uuid}`),
    enabled: !!uuid,
  });
}

export function usePlayerSnapshotAt(uuid: string | null, ts: string | null) {
  return useQuery({
    queryKey: ["data", "players", "at", uuid, ts],
    queryFn: () => apiGet<unknown>(`/players/${uuid}/at?ts=${encodeURIComponent(ts!)}`),
    enabled: !!uuid && !!ts,
  });
}

export function useGuildSnapshots(search: string, sort: string, offset: number, limit: number) {
  const params = new URLSearchParams();
  params.set("limit", String(limit));
  params.set("offset", String(offset));
  if (search) params.set("search", search);
  if (sort) params.set("sort", sort);

  return useQuery({
    queryKey: ["data", "guilds", params.toString()],
    queryFn: () => apiGet<GuildListResponse>(`/guilds?${params}`),
  });
}

export function useGuildDetail(guildId: string | null) {
  return useQuery({
    queryKey: ["data", "guilds", "detail", guildId],
    queryFn: () => apiGet<GuildDetail>(`/guilds/${guildId}`),
    enabled: !!guildId,
  });
}

export function useGuildAt(guildId: string | null, ts: string | null) {
  return useQuery({
    queryKey: ["data", "guilds", "at", guildId, ts],
    queryFn: () => apiGet<unknown>(`/guilds/${guildId}/at?ts=${encodeURIComponent(ts!)}`),
    enabled: !!guildId && !!ts,
  });
}

export function useResolveIds(uuids: string[], discordIds: string[]) {
  const params = new URLSearchParams();
  if (uuids.length > 0) params.set("uuids", uuids.join(","));
  if (discordIds.length > 0) params.set("discord", discordIds.join(","));

  return useQuery({
    queryKey: ["data", "resolve", params.toString()],
    queryFn: () => apiGet<ResolvedIds>(`/resolve?${params}`),
    enabled: uuids.length > 0 || discordIds.length > 0,
  });
}
