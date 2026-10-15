import type { LoaderFunctionArgs } from "react-router-dom";

import {
  accountActivityQuery,
  accountCoreQuery,
  accountListQuery,
  accountNetworkQuery,
  accountRequestsQuery,
  keyHistoryQuery,
  limitsQuery,
  lookupStreamQuery,
  lookupSummaryQuery,
  memberAccountQuery,
  starfishAccountQuery,
  starfishQuery,
  timelineQuery,
  topLookupsQuery,
} from "../../api/accounts";
import { queryClient } from "../../api/queryClient";
import { isSnowflake } from "../../lib/refs";
import { listFiltersFrom } from "./list-filters";
import { pickParam } from "./params";
import { ACCOUNT_TABS, ACTIVITY_WINDOWS, DEFAULT_ACTIVITY_WINDOW, DEFAULT_LOOKUP_WINDOW, DEFAULT_TAB, LOOKUP_WINDOWS, type AccountTab } from "./vocabulary";

const TAB_PREFETCH: Record<AccountTab, (discordId: string, search: URLSearchParams) => void> = {
  activity: (discordId, search) => {
    void queryClient.prefetchQuery(accountActivityQuery(discordId, pickParam(search, "window", ACTIVITY_WINDOWS, DEFAULT_ACTIVITY_WINDOW)));
    void queryClient.prefetchInfiniteQuery(accountRequestsQuery(discordId));
  },
  network: (discordId) => void queryClient.prefetchQuery(accountNetworkQuery(discordId)),
  lookups: (discordId, search) => {
    const window = pickParam(search, "lookups", LOOKUP_WINDOWS, DEFAULT_LOOKUP_WINDOW);
    void queryClient.prefetchQuery(lookupSummaryQuery(discordId, window));
    void queryClient.prefetchQuery(topLookupsQuery(discordId, window));
    void queryClient.prefetchInfiniteQuery(lookupStreamQuery(discordId));
  },
  moderation: (discordId) => void queryClient.prefetchInfiniteQuery(timelineQuery(discordId)),
  starfish: (discordId) => void queryClient.prefetchQuery(starfishQuery(discordId)),
  raw: () => undefined,
};

export function accountLoader({ params, request }: LoaderFunctionArgs): null {
  const discordId = params.discordId ?? "";
  if (!isSnowflake(discordId)) return null;
  const search = new URL(request.url).searchParams;
  void queryClient.prefetchQuery(accountCoreQuery(discordId));
  void queryClient.prefetchQuery(limitsQuery(discordId));
  void queryClient.prefetchQuery(keyHistoryQuery(discordId));
  TAB_PREFETCH[pickParam(search, "tab", ACCOUNT_TABS, DEFAULT_TAB)](discordId, search);
  return null;
}

export function accountsLoader({ request }: LoaderFunctionArgs): null {
  void queryClient.prefetchInfiniteQuery(accountListQuery(listFiltersFrom(new URL(request.url).searchParams)));
  return null;
}

export function memberRedirectLoader({ params }: LoaderFunctionArgs): null {
  void queryClient.prefetchQuery(memberAccountQuery(params.memberId ?? ""));
  return null;
}

export function starfishRedirectLoader({ params }: LoaderFunctionArgs): null {
  void queryClient.prefetchQuery(starfishAccountQuery(params.userId ?? ""));
  return null;
}
