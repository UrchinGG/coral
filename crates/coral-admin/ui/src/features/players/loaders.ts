import { redirect, type LoaderFunctionArgs } from "react-router-dom";

import { playerCoreQuery, playerRequestersQuery, playerRequestsQuery, playerSnapshotsQuery, playerTagsQuery, recentlyTaggedQuery, topPlayersQuery } from "../../api/players";
import { queryClient } from "../../api/queryClient";
import { normalizeUuid } from "../../lib/uuid";
import { canonicalPlayerLocation, parseForms, parseTopWindow, parseWindow } from "./player-state";

export function playerLoader({ params, request }: LoaderFunctionArgs) {
  const raw = params.uuid ?? "";
  const url = new URL(request.url);
  const canonical = canonicalPlayerLocation(raw, url.search);
  if (canonical !== null) return redirect(canonical);
  const uuid = normalizeUuid(raw);
  if (uuid === null) return null;
  const selection = { window: parseWindow(url.searchParams.get("window")), forms: parseForms(url.searchParams.get("forms")) };
  void queryClient.prefetchQuery(playerCoreQuery(uuid));
  void queryClient.prefetchQuery(playerRequestersQuery(uuid, selection));
  void queryClient.prefetchInfiniteQuery(playerRequestsQuery(uuid, selection, url.searchParams.get("requester")));
  void queryClient.prefetchQuery(playerTagsQuery(uuid));
  void queryClient.prefetchInfiniteQuery(playerSnapshotsQuery(uuid, url.searchParams.get("day")));
  return null;
}

export function playersLoader({ request }: LoaderFunctionArgs) {
  void queryClient.prefetchQuery(topPlayersQuery(parseTopWindow(new URL(request.url).searchParams.get("top"))));
  void queryClient.prefetchQuery(recentlyTaggedQuery());
  return null;
}
