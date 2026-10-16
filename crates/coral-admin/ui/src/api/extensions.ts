import { queryOptions } from "@tanstack/react-query";

import { postJson } from "./http";
import { keys } from "./keys";
import { entityQuery, getQuery } from "./queryClient";
import type { ActionResponse, ExtensionView } from "./types/extension";

const MS_PER_SECOND = 1_000;

export function extensionViewQuery(id: string) {
  return queryOptions({
    queryKey: keys.extensions.view(id),
    queryFn: getQuery<ExtensionView>(extensionPath(id)),
    ...entityQuery,
    refetchInterval: (query) => extensionRefreshMs(query.state.data),
  });
}

export function runExtensionAction(id: string, action: string, input: Record<string, unknown>): Promise<ActionResponse> {
  return postJson<ActionResponse>(`${extensionPath(id)}/${encodeURIComponent(action)}`, input);
}

export function extensionRefreshMs(view: ExtensionView | undefined): number | false {
  if (view === undefined || view.refresh_s === null) return false;
  return view.refresh_s * MS_PER_SECOND;
}

function extensionPath(id: string): string {
  return `/extensions/${encodeURIComponent(id)}`;
}
