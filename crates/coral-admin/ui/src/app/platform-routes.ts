import type { RouteObject } from "react-router-dom";

import type { PlatformPage, PlatformRedirect } from "../features/platform/routes";

const platform = () => import("../features/platform/routes");

export const PLATFORM_ROUTES: RouteObject[] = [
  page("/moderation", "moderation"),
  page("/server", "server"),
  page("/starfish", "starfish"),
  page("/plugins", "plugins"),
  page("/plugins/:slug", "plugin"),
  page("/guilds", "guilds"),
  page("/guilds/:guildId", "guild"),
  page("/tools/resolve", "resolve"),
  page("/maintenance", "maintenance"),
  page("/x/:id", "extension"),
];

export const REDIRECT_ROUTES: RouteObject[] = [
  moved("/diagnostics", "diagnostics"),
  moved("/servers", "servers"),
  moved("/starfish/plugins", "starfishPlugins"),
  moved("/starfish/plugins/:slug", "starfishPlugin"),
  moved("/data", "data"),
];

function page(path: string, name: PlatformPage): RouteObject {
  return {
    path,
    lazy: {
      Component: () => platform().then((module) => module.PAGES[name]()),
      loader: () => platform().then((module) => module.LOADERS[name]),
    },
  };
}

function moved(path: string, name: PlatformRedirect): RouteObject {
  return { path, lazy: { loader: () => platform().then((module) => module.REDIRECTS[name]) } };
}
