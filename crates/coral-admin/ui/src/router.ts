import { createBrowserRouter, redirect, type RouteObject } from "react-router-dom";

import { NotFound } from "./app/NotFound";
import { PLATFORM_ROUTES, REDIRECT_ROUTES } from "./app/platform-routes";
import { RouteError, ShellError } from "./app/RouteError";
import { Shell } from "./app/Shell";
import { ShellSkeleton } from "./app/ShellSkeleton";

const FEATURE_ROUTES: RouteObject[] = [
  {
    path: "/",
    lazy: {
      Component: () => import("./features/home/HomePage").then((module) => module.HomePage),
      loader: () => import("./features/home/loader").then((module) => module.homeLoader),
    },
  },
  {
    path: "/traffic",
    lazy: {
      Component: () => import("./features/traffic/TrafficPage").then((module) => module.TrafficPage),
      loader: () => import("./features/traffic/loader").then((module) => module.trafficLoader),
    },
  },
  { path: "/search", lazy: { Component: () => import("./features/search/SearchPage").then((module) => module.SearchPage) } },
  {
    path: "/requests",
    lazy: {
      Component: () => import("./features/requests/RequestsPage").then((module) => module.RequestsPage),
      loader: () => import("./features/requests/explorer-loader").then((module) => module.explorerLoader),
      shouldRevalidate: () => import("./features/requests/explorer-loader").then((module) => module.explorerShouldRevalidate),
    },
  },
  {
    path: "/requests/:requestId",
    lazy: {
      Component: () => import("./features/requests/RequestPage").then((module) => module.RequestPage),
      loader: () => import("./features/requests/loaders").then((module) => module.requestLoader),
    },
  },
  { path: "/dev/kit", lazy: { Component: () => import("./features/dev-kit/KitGallery").then((module) => module.KitGallery) } },
  {
    path: "/accounts",
    lazy: {
      Component: () => import("./features/accounts/AccountsPage").then((module) => module.AccountsPage),
      loader: () => import("./features/accounts/loader").then((module) => module.accountsLoader),
    },
  },
  {
    path: "/accounts/by-member/:memberId",
    lazy: {
      Component: () => import("./features/accounts/AccountRedirects").then((module) => module.MemberRedirect),
      loader: () => import("./features/accounts/loader").then((module) => module.memberRedirectLoader),
    },
  },
  {
    path: "/accounts/:discordId",
    lazy: {
      Component: () => import("./features/accounts/AccountPage").then((module) => module.AccountPage),
      loader: () => import("./features/accounts/loader").then((module) => module.accountLoader),
    },
  },
  { path: "/members/:memberId?", loader: ({ params }) => redirect(params.memberId === undefined ? "/accounts" : `/accounts/by-member/${encodeURIComponent(params.memberId)}`) },
  {
    path: "/starfish/users/:userId",
    lazy: {
      Component: () => import("./features/accounts/AccountRedirects").then((module) => module.StarfishRedirect),
      loader: () => import("./features/accounts/loader").then((module) => module.starfishRedirectLoader),
    },
  },
  {
    path: "/ips/:ip",
    lazy: { Component: () => import("./features/ips/IpPage").then((module) => module.IpPage), loader: () => import("./features/ips/loader").then((module) => module.ipLoader) },
  },
  {
    path: "/keys/*",
    lazy: { Component: () => import("./features/keys/KeyPage").then((module) => module.KeyPage), loader: () => import("./features/keys/loader").then((module) => module.keyLoader) },
  },
  {
    path: "/players",
    lazy: {
      Component: () => import("./features/players/PlayersPage").then((module) => module.PlayersPage),
      loader: () => import("./features/players/loaders").then((module) => module.playersLoader),
    },
  },
  {
    path: "/players/:uuid",
    lazy: {
      Component: () => import("./features/players/PlayerPage").then((module) => module.PlayerPage),
      loader: () => import("./features/players/loaders").then((module) => module.playerLoader),
    },
  },
];

export const router = createBrowserRouter([
  {
    Component: Shell,
    ErrorBoundary: ShellError,
    HydrateFallback: ShellSkeleton,
    children: [
      {
        ErrorBoundary: RouteError,
        children: [...FEATURE_ROUTES, ...PLATFORM_ROUTES, ...REDIRECT_ROUTES, { path: "*", Component: NotFound }],
      },
    ],
  },
]);
