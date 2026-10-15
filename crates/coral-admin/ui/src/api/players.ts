import { infiniteQueryOptions, keepPreviousData, queryOptions, useMutation, useQueryClient } from "@tanstack/react-query";

import { deleteJson, getJson, postJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { entityQuery, immutableQuery, infiniteListQuery, STALE_LIST_MS } from "./queryClient";
import type {
  NewTag,
  PlayerCore,
  PlayerForm,
  PlayerRequesters,
  PlayerRequestRow,
  PlayerTags,
  PlayerWindow,
  SnapshotAt,
  RecentlyTagged,
  SnapshotPage,
  TopPlayers,
  TopWindow,
} from "./types/players";
import type { Page } from "./types/refs";

const FORM_SEPARATOR = ",";

const playerKeys = {
  requesters: (uuid: string, window: string, forms: string) => [...keys.players.of(uuid), "requesters", window, forms] as const,
  requests: (uuid: string, window: string, forms: string, requester: string | null) => [...keys.players.of(uuid), "requests", window, forms, requester] as const,
  tags: (uuid: string) => [...keys.players.of(uuid), "tags"] as const,
  snapshots: (uuid: string, day: string | null) => [...keys.players.of(uuid), "snapshots", day] as const,
  snapshotAt: (uuid: string, ts: string) => [...keys.players.of(uuid), "snapshot-at", ts] as const,
  top: (window: string) => [...keys.players.lists(), "top", window] as const,
  recentlyTagged: () => [...keys.players.lists(), "recently-tagged"] as const,
};

export type LookupSelection = { window: PlayerWindow; forms: readonly PlayerForm[] };

export function playerCoreQuery(uuid: string) {
  return queryOptions({
    queryKey: keys.players.core(uuid),
    queryFn: ({ signal, queryKey }) => getJson<PlayerCore>(`/players/${uuid}`, { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function playerRequestersQuery(uuid: string, selection: LookupSelection) {
  const forms = formsParam(selection.forms);
  return queryOptions({
    queryKey: playerKeys.requesters(uuid, selection.window, forms),
    queryFn: ({ signal, queryKey }) =>
      getJson<PlayerRequesters>(`/players/${uuid}/requesters?${new URLSearchParams({ window: selection.window, forms })}`, { signal, timingKey: queryKey }).then(
        withLabels,
      ),
    staleTime: STALE_LIST_MS,
  });
}

export function playerRequestsQuery(uuid: string, selection: LookupSelection, requester: string | null) {
  const forms = formsParam(selection.forms);
  return infiniteQueryOptions({
    queryKey: playerKeys.requests(uuid, selection.window, forms, requester),
    queryFn: ({ signal, queryKey, pageParam }) => {
      const params = new URLSearchParams({ window: selection.window, forms });
      if (requester !== null) params.set("requester", requester);
      if (pageParam !== null) params.set("cursor", pageParam);
      return getJson<Page<PlayerRequestRow>>(`/players/${uuid}/requests?${params}`, { signal, timingKey: queryKey }).then(withLabels);
    },
    staleTime: STALE_LIST_MS,
    placeholderData: keepPreviousData,
    initialPageParam: infiniteListQuery.initialPageParam,
    getNextPageParam: (page: Page<PlayerRequestRow>) => page.next_cursor,
  });
}

export function playerTagsQuery(uuid: string) {
  return queryOptions({
    queryKey: playerKeys.tags(uuid),
    queryFn: ({ signal, queryKey }) => getJson<PlayerTags>(`/players/${uuid}/tags`, { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function playerSnapshotsQuery(uuid: string, day: string | null) {
  return infiniteQueryOptions({
    queryKey: playerKeys.snapshots(uuid, day),
    queryFn: ({ signal, queryKey, pageParam }) => {
      const params = new URLSearchParams();
      if (day !== null) params.set("day", day);
      if (pageParam !== null) params.set("cursor", pageParam);
      return getJson<SnapshotPage>(`/players/${uuid}/snapshots?${params}`, { signal, timingKey: queryKey });
    },
    staleTime: STALE_LIST_MS,
    placeholderData: keepPreviousData,
    initialPageParam: infiniteListQuery.initialPageParam,
    getNextPageParam: (page: SnapshotPage) => page.next_cursor,
  });
}

export function snapshotAtQuery(uuid: string, ts: string) {
  return queryOptions({
    queryKey: playerKeys.snapshotAt(uuid, ts),
    queryFn: ({ signal, queryKey }) => getJson<SnapshotAt>(`/players/${uuid}/snapshots/at?${new URLSearchParams({ ts })}`, { signal, timingKey: queryKey }),
    ...immutableQuery,
  });
}

export function topPlayersQuery(window: TopWindow) {
  return queryOptions({
    queryKey: playerKeys.top(window),
    queryFn: ({ signal, queryKey }) => getJson<TopPlayers>(`/players/top?${new URLSearchParams({ window })}`, { signal, timingKey: queryKey }).then(withLabels),
    staleTime: STALE_LIST_MS,
  });
}

export function recentlyTaggedQuery() {
  return queryOptions({
    queryKey: playerKeys.recentlyTagged(),
    queryFn: ({ signal, queryKey }) => getJson<RecentlyTagged>("/players/recently-tagged", { signal, timingKey: queryKey }).then(withLabels),
    staleTime: STALE_LIST_MS,
  });
}

export function usePlayerMutations(uuid: string) {
  const queryClient = useQueryClient();
  const settle = () =>
    Promise.all([
      queryClient.invalidateQueries({ queryKey: keys.players.core(uuid) }),
      queryClient.invalidateQueries({ queryKey: playerKeys.tags(uuid) }),
      queryClient.invalidateQueries({ queryKey: playerKeys.recentlyTagged() }),
    ]);
  const base = `/players/${uuid}`;
  return {
    addTag: useMutation({ mutationFn: (tag: NewTag) => postJson<unknown>(`${base}/tags`, tag), onSuccess: settle }),
    removeTag: useMutation({ mutationFn: (tagType: string) => deleteJson<unknown>(`${base}/tags/${encodeURIComponent(tagType)}`), onSuccess: settle }),
    lock: useMutation({ mutationFn: (reason: string | null) => postJson<unknown>(`${base}/lock`, { reason }), onSuccess: settle }),
    unlock: useMutation({ mutationFn: () => postJson<unknown>(`${base}/unlock`, {}), onSuccess: settle }),
  };
}

function formsParam(forms: readonly PlayerForm[]): string {
  return [...forms].sort().join(FORM_SEPARATOR);
}
