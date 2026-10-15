import { queryOptions } from "@tanstack/react-query";

import { deleteJson, getJson, postJson } from "./http";
import { keys } from "./keys";
import { withLabels } from "./labels";
import { entityQuery } from "./queryClient";
import { rowPages } from "./row-pages";
import type { PluginDetail, PluginRow } from "./types/platform";

export type PluginFlag = "official" | "unlisted" | "disabled";

export type FlagChange = { flag: PluginFlag; on: boolean; reason?: string };

export function pluginsQuery(q: string) {
  const params = new URLSearchParams();
  if (q !== "") params.set("q", q);
  return rowPages<PluginRow>(keys.plugins.list(q), "/plugins", params);
}

export function pluginQuery(slug: string) {
  return queryOptions({
    queryKey: keys.plugins.core(slug),
    queryFn: ({ signal, queryKey }) => getJson<PluginDetail>(pluginPath(slug), { signal, timingKey: queryKey }).then(withLabels),
    ...entityQuery,
  });
}

export function setPluginFlag(slug: string, change: FlagChange): Promise<void> {
  const body = change.flag === "disabled" ? { disabled: change.on, reason: change.reason ?? null } : { [change.flag]: change.on };
  return postJson<void>(`${pluginPath(slug)}/${change.flag}`, body);
}

export function deletePlugin(slug: string): Promise<void> {
  return deleteJson<void>(pluginPath(slug));
}

export function yankRelease(slug: string, version: string, reason: string): Promise<void> {
  return postJson<void>(`${releasePath(slug, version)}/yank`, { reason: reason === "" ? null : reason });
}

export function unyankRelease(slug: string, version: string): Promise<void> {
  return postJson<void>(`${releasePath(slug, version)}/unyank`, {});
}

export function deleteRelease(slug: string, version: string): Promise<void> {
  return deleteJson<void>(releasePath(slug, version));
}

export function deleteReview(slug: string, userId: number): Promise<void> {
  return deleteJson<void>(`${pluginPath(slug)}/reviews/${userId}`);
}

function pluginPath(slug: string): string {
  return `/plugins/${encodeURIComponent(slug)}`;
}

function releasePath(slug: string, version: string): string {
  return `${pluginPath(slug)}/releases/${encodeURIComponent(version)}`;
}
