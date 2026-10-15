import { useSyncExternalStore } from "react";

import { budgetFor } from "./budgets";
import { isApiError } from "./http";
import { queryClient } from "./queryClient";
import { subscribeTimings, timingOf, type QueryTiming } from "./timing";

export type PageQuery = {
  hash: string;
  label: string;
  status: "pending" | "error" | "success";
  fetching: boolean;
  timing: QueryTiming | null;
  budgetMs: number | null;
  errorCode: string | null;
};

const RELEVANT_EVENTS = new Set(["added", "removed", "updated", "observerAdded", "observerRemoved"]);

const listeners = new Set<() => void>();
let snapshot: PageQuery[] = [];
let signature = "";
let recomputeQueued = false;
let watching = false;

export function usePageQueries(): PageQuery[] {
  return useSyncExternalStore(subscribe, () => snapshot);
}

export function slowestOf(queries: readonly PageQuery[]): PageQuery | null {
  return queries.reduce<PageQuery | null>((slowest, query) => (serverMsOf(query) > serverMsOf(slowest) ? query : slowest), null);
}

export function serverMsOf(query: PageQuery | null): number {
  return query?.timing?.serverMs ?? -1;
}

export function isOverBudget(query: PageQuery): boolean {
  return query.budgetMs !== null && serverMsOf(query) > query.budgetMs;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  startWatching();
  return () => listeners.delete(listener);
}

function startWatching(): void {
  if (watching) return;
  watching = true;
  queryClient.getQueryCache().subscribe((event) => {
    if (RELEVANT_EVENTS.has(event.type)) queueRecompute();
  });
  subscribeTimings(queueRecompute);
  recompute();
}

function queueRecompute(): void {
  if (recomputeQueued) return;
  recomputeQueued = true;
  queueMicrotask(recompute);
}

function recompute(): void {
  recomputeQueued = false;
  const next = queryClient
    .getQueryCache()
    .getAll()
    .filter((query) => query.getObserversCount() > 0 && !query.isDisabled())
    .map((query) => {
      const timing = timingOf(query.queryHash);
      const error = query.state.error;
      return {
        hash: query.queryHash,
        label: timing?.endpoint ?? query.queryHash,
        status: query.state.status,
        fetching: query.state.fetchStatus === "fetching",
        timing,
        budgetMs: timing === null ? null : budgetFor(timing.endpoint),
        errorCode: isApiError(error) ? `${error.status} ${error.code}` : error === null ? null : error.message,
      };
    });
  const nextSignature = JSON.stringify(next);
  if (nextSignature === signature) return;
  signature = nextSignature;
  snapshot = next;
  listeners.forEach((listener) => listener());
}
