import type { LoaderFunctionArgs, ShouldRevalidateFunctionArgs } from "react-router-dom";

import { queryClient } from "../../api/queryClient";
import { requestGroupsQuery, requestListQuery, requestRoutesQuery } from "../../api/requests";
import { readHistoryValue } from "../../lib/history-state";
import { REQUEST_TEXT_STATE_KEY } from "./paths";
import { rememberLoaderAnchor } from "./explorer-anchor";
import { explorerViewFrom, filterFromParams, groupGaps, groupQueryOf, groupSortFrom, histogramFilterOf, histogramGaps, requestQueryOf, windowBounds } from "./explorer-filter";
import { histogramQuery } from "./histogram";

export function explorerLoader({ request }: LoaderFunctionArgs): null {
  const params = new URL(request.url).searchParams;
  const filter = filterFromParams(params);
  const q = readHistoryValue(REQUEST_TEXT_STATE_KEY) ?? "";
  const at = Date.now();
  const bounds = windowBounds(filter.range, at);
  rememberLoaderAnchor(filter, at);
  void queryClient.prefetchQuery(requestRoutesQuery());
  if (histogramGaps(filter, q).length === 0) void queryClient.prefetchQuery(histogramQuery(histogramFilterOf(filter, bounds, at)));
  if (explorerViewFrom(params) === "list") void queryClient.prefetchInfiniteQuery(requestListQuery(requestQueryOf(filter, q, bounds)));
  else if (groupGaps(filter, q).length === 0) void queryClient.prefetchQuery(requestGroupsQuery(groupQueryOf(filter, bounds, at, groupSortFrom(params))));
  return null;
}

export function explorerShouldRevalidate({ currentUrl, nextUrl }: ShouldRevalidateFunctionArgs): boolean {
  return currentUrl.pathname !== nextUrl.pathname;
}
