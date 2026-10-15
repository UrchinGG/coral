import type { ComponentType } from "react";
import { redirect, type LoaderFunction, type LoaderFunctionArgs } from "react-router-dom";

import { extensionViewQuery } from "../../api/extensions";
import { guildQuery, guildSnapshotQuery, guildsQuery } from "../../api/guilds";
import { maintenanceQuery } from "../../api/maintenance";
import { auditQuery, blacklistQuery, guideQuery, rulesQuery, strikesQuery, tagTypesQuery } from "../../api/moderation";
import { pluginQuery, pluginsQuery } from "../../api/plugins";
import { queryClient } from "../../api/queryClient";
import { directoryQuery, populationQuery, serverConfigQuery, syncJobsQuery } from "../../api/server";
import { starfishUsersQuery } from "../../api/starfish";
import { guildSortOf } from "../guilds/params";
import { blacklistFiltersOf, strikeFilterOf, tabOf } from "../moderation/params";

export type PlatformPage = keyof typeof PAGES;

export type PlatformRedirect = keyof typeof REDIRECTS;

const REQUEST_LOG_PARAMS = ["path", "path_exact", "method", "status", "key_prefix", "ip", "discord_id", "caller", "error_contains", "errors", "q", "from", "to", "offset"] as const;

export const PAGES = {
  moderation: () => import("../moderation/ModerationPage").then((module) => module.ModerationPage),
  server: () => import("../server/ServerPage").then((module) => module.ServerPage),
  starfish: () => import("../starfish/StarfishPage").then((module) => module.StarfishPage),
  plugins: () => import("../plugins/PluginsPage").then((module) => module.PluginsPage),
  plugin: () => import("../plugins/PluginPage").then((module) => module.PluginPage),
  guilds: () => import("../guilds/GuildsPage").then((module) => module.GuildsPage),
  guild: () => import("../guilds/GuildPage").then((module) => module.GuildPage),
  resolve: () => import("../tools/ResolvePage").then((module) => module.ResolvePage),
  maintenance: () => import("../maintenance/MaintenancePage").then((module) => module.MaintenancePage),
  extension: () => import("../extension/ExtensionPage").then((module) => module.ExtensionPage),
} satisfies Record<string, () => Promise<ComponentType>>;

export const LOADERS: Partial<Record<PlatformPage, LoaderFunction>> = {
  moderation: moderationLoader,
  server: serverLoader,
  starfish: starfishLoader,
  plugins: pluginsLoader,
  plugin: pluginLoader,
  guilds: guildsLoader,
  guild: guildLoader,
  maintenance: maintenanceLoader,
  extension: extensionLoader,
};

export const REDIRECTS = {
  diagnostics: diagnosticsRedirect,
  servers: () => redirect("/server"),
  starfishPlugins: () => redirect("/plugins"),
  starfishPlugin: ({ params }: LoaderFunctionArgs) => redirect(`/plugins/${encodeURIComponent(params.slug ?? "")}`),
  data: () => redirect("/guilds"),
} satisfies Record<string, LoaderFunction>;

function moderationLoader({ request }: LoaderFunctionArgs): null {
  const params = searchOf(request);
  switch (tabOf(params)) {
    case "blacklist":
      void queryClient.prefetchQuery(tagTypesQuery());
      void queryClient.prefetchInfiniteQuery(blacklistQuery(blacklistFiltersOf(params)));
      return null;
    case "strikes":
      void queryClient.prefetchInfiniteQuery(strikesQuery(strikeFilterOf(params)));
      return null;
    case "audit":
      void queryClient.prefetchInfiniteQuery(auditQuery(params.get("action") ?? ""));
      return null;
    case "guide":
      void queryClient.prefetchQuery(guideQuery());
      void queryClient.prefetchQuery(rulesQuery());
      prefetchDiscord();
      return null;
  }
}

function serverLoader(): null {
  void queryClient.prefetchQuery(serverConfigQuery());
  void queryClient.prefetchQuery(syncJobsQuery());
  prefetchDiscord();
  return null;
}

function starfishLoader({ request }: LoaderFunctionArgs): null {
  void queryClient.prefetchInfiniteQuery(starfishUsersQuery(searchOf(request).get("q") ?? ""));
  return null;
}

function pluginsLoader({ request }: LoaderFunctionArgs): null {
  void queryClient.prefetchInfiniteQuery(pluginsQuery(searchOf(request).get("q") ?? ""));
  return null;
}

function pluginLoader({ params }: LoaderFunctionArgs): null {
  if (params.slug !== undefined) void queryClient.prefetchQuery(pluginQuery(params.slug));
  return null;
}

function guildsLoader({ request }: LoaderFunctionArgs): null {
  const params = searchOf(request);
  void queryClient.prefetchInfiniteQuery(guildsQuery(params.get("q") ?? "", guildSortOf(params)));
  return null;
}

function guildLoader({ params, request }: LoaderFunctionArgs): null {
  if (params.guildId === undefined) return null;
  void queryClient.prefetchQuery(guildQuery(params.guildId));
  const at = searchOf(request).get("at");
  if (at !== null) void queryClient.prefetchQuery(guildSnapshotQuery(params.guildId, at));
  return null;
}

function maintenanceLoader(): null {
  void queryClient.prefetchQuery(maintenanceQuery());
  return null;
}

function extensionLoader({ params }: LoaderFunctionArgs): null {
  if (params.id !== undefined) void queryClient.prefetchQuery(extensionViewQuery(params.id));
  return null;
}

function diagnosticsRedirect({ request }: LoaderFunctionArgs): Response {
  const params = searchOf(request);
  return redirect(REQUEST_LOG_PARAMS.some((name) => params.has(name)) ? "/requests" : "/traffic");
}

function prefetchDiscord(): void {
  void queryClient.prefetchQuery(directoryQuery());
  void queryClient.prefetchQuery(populationQuery());
}

function searchOf(request: Request): URLSearchParams {
  return new URL(request.url).searchParams;
}
