import { infiniteQueryOptions, keepPreviousData, queryOptions, type QueryClient } from "@tanstack/react-query";

import { deleteJson, getJson, postJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { cursorPaging } from "./paging";
import { entityQuery, listQuery, liveQuery, POLL_LIMITS_MS, STALE_LIST_MS } from "./queryClient";
import type {
  AccountActivity,
  AccountCore,
  AccountListFilters,
  AccountNetwork,
  AccountRow,
  ActivityWindow,
  Budget,
  CursorPage,
  DevKey,
  DevKeyInput,
  KeyRecord,
  LinkRecord,
  LookupRow,
  LookupSummary,
  LookupWindow,
  ResetLimits,
  StarfishAccount,
  StrikeInput,
  TimelineEvent,
  TopLookups,
} from "./types/accounts";
import type { Labels, Page } from "./types/refs";
import type { RequestRow } from "./types/requests";

type AccountSection = "activity" | "network" | "lookups-summary" | "lookups-top" | "lookups" | "timeline" | "keys" | "links" | "limits" | "starfish" | "requests";

export const LICENSE_STATUSES = ["inactive", "active", "suspended"] as const;
export type LicenseStatus = (typeof LICENSE_STATUSES)[number];

export const accountKeys = {
  list: (filters: AccountListFilters) => [...keys.accounts.all(), "list", filters] as const,
  sections: (discordId: string) => [...keys.accounts.all(), "section", discordId] as const,
  section: (discordId: string, section: AccountSection, ...params: readonly string[]) => [...accountKeys.sections(discordId), section, ...params] as const,
  byMember: (memberId: string) => [...keys.accounts.all(), "by-member", memberId] as const,
  byStarfish: (userId: string) => [...keys.accounts.all(), "by-starfish", userId] as const,
};

export function accountListQuery(filters: AccountListFilters) {
  return infiniteQueryOptions({
    queryKey: accountKeys.list(filters),
    queryFn: ({ signal, queryKey, pageParam }) =>
      getJson<CursorPage<AccountRow>>(`/accounts?${accountListParams(filters, pageParam)}`, { signal, timingKey: queryKey }).then(withLabels),
    ...cursorPaging<CursorPage<AccountRow>>(),
    placeholderData: keepPreviousData,
  });
}

export function accountCoreQuery(discordId: string) {
  return queryOptions({
    queryKey: keys.accounts.core(discordId),
    queryFn: ({ signal, queryKey }) => getJson<AccountCore>(accountPath(discordId), { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function memberAccountQuery(memberId: string) {
  return queryOptions({
    queryKey: accountKeys.byMember(memberId),
    queryFn: ({ signal, queryKey }) => getJson<{ discord_id: string }>(`/accounts/by-member/${encodeURIComponent(memberId)}`, { signal, timingKey: queryKey }),
    ...entityQuery,
    retry: false,
  });
}

export function starfishAccountQuery(userId: string) {
  return queryOptions({
    queryKey: accountKeys.byStarfish(userId),
    queryFn: ({ signal, queryKey }) => getJson<{ discord_id: string }>(`/accounts/by-starfish/${encodeURIComponent(userId)}`, { signal, timingKey: queryKey }),
    ...entityQuery,
    retry: false,
  });
}

export function accountActivityQuery(discordId: string, window: ActivityWindow) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, "activity", window),
    queryFn: ({ signal, queryKey }) => getJson<AccountActivity>(accountPath(discordId, `/activity?${new URLSearchParams({ window })}`), { signal, timingKey: queryKey }),
    ...listQuery,
  });
}

export function accountNetworkQuery(discordId: string) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, "network"),
    queryFn: ({ signal, queryKey }) => getJson<AccountNetwork>(accountPath(discordId, "/network"), { signal, timingKey: queryKey }).then(withLabels),
    staleTime: STALE_LIST_MS,
  });
}

export function lookupSummaryQuery(discordId: string, window: LookupWindow) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, "lookups-summary", window),
    queryFn: ({ signal, queryKey }) => getJson<LookupSummary>(accountPath(discordId, `/lookups/summary?${new URLSearchParams({ window })}`), { signal, timingKey: queryKey }),
    ...listQuery,
  });
}

export function topLookupsQuery(discordId: string, window: LookupWindow) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, "lookups-top", window),
    queryFn: async ({ signal, queryKey }): Promise<TopLookups> =>
      withLabels(await getJson<TopLookups>(accountPath(discordId, `/lookups/top?${new URLSearchParams({ window })}`), { signal, timingKey: queryKey })),
    ...listQuery,
  });
}

export function lookupStreamQuery(discordId: string) {
  return cursorQuery<CursorPage<LookupRow>>(discordId, "lookups", "/lookups");
}

export function timelineQuery(discordId: string) {
  return cursorQuery<CursorPage<TimelineEvent>>(discordId, "timeline", "/timeline");
}

export function accountRequestsQuery(discordId: string) {
  return cursorQuery<Page<RequestRow>>(discordId, "requests", "/requests");
}

export function keyHistoryQuery(discordId: string) {
  return sectionQuery<KeyRecord[]>(discordId, "keys", "/keys");
}

export function linkHistoryQuery(discordId: string) {
  return sectionQuery<LinkRecord[]>(discordId, "links", "/links");
}

export function limitsQuery(discordId: string) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, "limits"),
    queryFn: ({ signal, queryKey }) => getJson<Budget[]>(accountPath(discordId, "/limits"), { signal, timingKey: queryKey }),
    ...liveQuery(POLL_LIMITS_MS),
  });
}

export function starfishQuery(discordId: string) {
  return sectionQuery<StarfishAccount | null>(discordId, "starfish", "/starfish");
}

export const accountActions = {
  lock: (discordId: string) => postJson<void>(accountPath(discordId, "/lock"), {}),
  unlock: (discordId: string) => postJson<void>(accountPath(discordId, "/unlock"), {}),
  setAccessLevel: (discordId: string, level: number) => postJson<void>(accountPath(discordId, "/access-level"), { level }),
  setTagging: (discordId: string, disabled: boolean) => postJson<void>(accountPath(discordId, "/tagging"), { disabled }),
  addStrike: (discordId: string, strike: StrikeInput) => postJson<void>(accountPath(discordId, "/strikes"), strike),
  revokeStrike: (discordId: string, strikeId: number) => deleteJson<void>(accountPath(discordId, `/strikes/${strikeId}`)),
  regenerateKey: (discordId: string) => postJson<{ api_key: string }>(accountPath(discordId, "/api-key/regenerate"), {}),
  createDevKey: (discordId: string, input: DevKeyInput) => postJson<DevKey>(accountPath(discordId, "/dev-key"), input),
  deleteDevKey: (discordId: string) => deleteJson<void>(accountPath(discordId, "/dev-key")),
  setDevKeyLocked: (discordId: string, locked: boolean) => postJson<void>(accountPath(discordId, "/dev-key/lock"), { locked }),
  setDevKeyRateLimit: (discordId: string, rateLimit: number) => postJson<void>(accountPath(discordId, "/dev-key/rate-limit"), { rate_limit: rateLimit }),
  setDevKeyPermissions: (discordId: string, permissions: number) => postJson<void>(accountPath(discordId, "/dev-key/permissions"), { permissions }),
  resetLimits: (discordId: string) => postJson<ResetLimits>(accountPath(discordId, "/limits/reset"), {}),
  setLicense: (discordId: string, status: LicenseStatus) => postJson<void>(accountPath(discordId, "/starfish/license"), { status }),
  revokeSessions: (discordId: string) => postJson<void>(accountPath(discordId, "/starfish/sessions/revoke"), {}),
};

export function invalidateAccount(client: QueryClient, discordId: string): Promise<void> {
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.accounts.core(discordId) }),
    client.invalidateQueries({ queryKey: accountKeys.sections(discordId) }),
    client.invalidateQueries({ queryKey: [...keys.accounts.all(), "list"] }),
  ]).then(() => undefined);
}

export function accountListParams(filters: AccountListFilters, cursor: string | null): URLSearchParams {
  const params = new URLSearchParams({ sort: filters.sort, dir: filters.dir });
  const optional: [string, number | boolean | null][] = [
    ["rank", filters.rank],
    ["locked", filters.locked],
    ["has_key", filters.has_key],
    ["has_dev_key", filters.has_dev_key],
    ["starfish", filters.starfish],
  ];
  for (const [name, value] of optional) if (value !== null) params.set(name, String(value));
  if (cursor !== null) params.set("cursor", cursor);
  return params;
}

function sectionQuery<T>(discordId: string, section: AccountSection, path: string) {
  return queryOptions({
    queryKey: accountKeys.section(discordId, section),
    queryFn: ({ signal, queryKey }) => getJson<T>(accountPath(discordId, path), { signal, timingKey: queryKey }),
    staleTime: STALE_LIST_MS,
  });
}

function cursorQuery<T extends { next_cursor: string | null; labels: Labels }>(discordId: string, section: AccountSection, path: string) {
  return infiniteQueryOptions({
    queryKey: accountKeys.section(discordId, section),
    queryFn: ({ signal, queryKey, pageParam }) =>
      getJson<T>(accountPath(discordId, `${path}${cursorSuffix(pageParam)}`), { signal, timingKey: queryKey }).then(withLabels),
    ...cursorPaging<T>(),
  });
}

function accountPath(discordId: string, rest = ""): string {
  return `/accounts/${encodeURIComponent(discordId)}${rest}`;
}

function cursorSuffix(cursor: string | null): string {
  return cursor === null ? "" : `?${new URLSearchParams({ cursor })}`;
}
