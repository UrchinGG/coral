import { queryOptions } from "@tanstack/react-query";

import { postJson } from "./http";
import { keys } from "./keys";
import { entityQuery, getQuery } from "./queryClient";
import type { ActionResponse, ExtensionView } from "./types/extension";

export function extensionViewQuery(id: string) {
  return queryOptions({ queryKey: keys.extensions.view(id), queryFn: getQuery<ExtensionView>(extensionPath(id)), ...entityQuery });
}

export function runExtensionAction(id: string, action: string, input: Record<string, unknown>): Promise<ActionResponse> {
  return postJson<ActionResponse>(`${extensionPath(id)}/${encodeURIComponent(action)}`, input);
}

function extensionPath(id: string): string {
  return `/extensions/${encodeURIComponent(id)}`;
}
