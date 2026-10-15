import { useCallback, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";

import type { GroupSort } from "../../api/types/request-explorer";
import { currentRouterState, flushHistoryWrites, useHistoryValue } from "../../lib/history-state";
import { REQUEST_TEXT_STATE_KEY } from "./paths";
import { loaderAnchorFor } from "./explorer-anchor";
import {
  DEFAULT_GROUP_SORT,
  explorerViewFrom,
  filterFromParams,
  groupSortFrom,
  SORT_PARAM,
  VIEW_PARAM,
  windowBounds,
  withFilterParams,
  type ExplorerFilter,
  type ExplorerView,
  type WindowBounds,
} from "./explorer-filter";

export type ExplorerState = {
  filter: ExplorerFilter;
  q: string;
  view: ExplorerView;
  live: boolean;
  selected: string | null;
  sort: GroupSort;
  bounds: WindowBounds;
  anchorMs: number;
  setFilter: (filter: ExplorerFilter, view?: ExplorerView) => void;
  setQ: (q: string) => void;
  setView: (view: ExplorerView) => void;
  setLive: (live: boolean) => void;
  setSelected: (requestId: string | null) => void;
  setSort: (sort: GroupSort) => void;
  refresh: () => void;
};

type Anchor = { signature: string; at: number };

const FLAG_ON = "1";
const LIVE_PARAM = "live";
const SELECTED_PARAM = "r";

export function useExplorerState(): ExplorerState {
  const [params, setParams] = useSearchParams();
  const [q, setQ] = useHistoryValue(REQUEST_TEXT_STATE_KEY);
  const filter = useMemo(() => filterFromParams(params), [params]);
  const signature = JSON.stringify([filter, q.trim()]);
  const [anchor, setAnchor] = useState<Anchor>(() => ({ signature, at: loaderAnchorFor(filter) ?? Date.now() }));
  const current = anchor.signature === signature ? anchor : { signature, at: Date.now() };
  if (current !== anchor) setAnchor(current);

  const update = useCallback(
    (change: (next: URLSearchParams) => URLSearchParams) => {
      flushHistoryWrites();
      setParams((previous) => change(new URLSearchParams(previous)), { replace: true, state: currentRouterState() });
    },
    [setParams],
  );
  const setParam = useCallback(
    (name: string, value: string | null) =>
      update((next) => {
        if (value === null) next.delete(name);
        else next.set(name, value);
        return next;
      }),
    [update],
  );

  return {
    filter,
    q,
    view: explorerViewFrom(params),
    live: params.get(LIVE_PARAM) === FLAG_ON,
    selected: params.get(SELECTED_PARAM),
    sort: groupSortFrom(params),
    bounds: windowBounds(filter.range, current.at),
    anchorMs: current.at,
    setFilter: (next, view) =>
      update((base) => {
        const withFilter = withFilterParams(base, next);
        withFilter.delete(SELECTED_PARAM);
        if (next.range.kind === "custom") withFilter.delete(LIVE_PARAM);
        if (view === "list") withFilter.delete(VIEW_PARAM);
        return withFilter;
      }),
    setQ,
    setView: (view) => setParam(VIEW_PARAM, view === "list" ? null : view),
    setLive: (live) => setParam(LIVE_PARAM, live ? FLAG_ON : null),
    setSelected: (requestId) => setParam(SELECTED_PARAM, requestId),
    setSort: (sort) => setParam(SORT_PARAM, sort === DEFAULT_GROUP_SORT ? null : sort),
    refresh: () => setAnchor({ signature, at: Date.now() }),
  };
}
