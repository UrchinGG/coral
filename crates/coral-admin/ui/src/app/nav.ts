import {
  Activity,
  Box,
  Flag,
  HardDrive,
  House,
  List,
  Plug,
  Puzzle,
  Server,
  Shield,
  Star,
  Users,
  Wrench,
  type LucideIcon,
} from "lucide-react";

import type { BootExtension } from "../api/types/session";
import type { PageJump } from "../omnibox/useOmnibox";

export type NavItem = {
  label: string;
  path: string;
  icon: LucideIcon;
  shortcut: string | null;
  matches: readonly string[];
  badge: string | null;
};

export type NavGroup = { title: string | null; items: readonly NavItem[] };

export const GO_SHORTCUTS: Readonly<Record<string, string>> = {
  h: "/",
  r: "/requests",
  t: "/traffic",
  a: "/accounts",
  p: "/players",
  m: "/moderation",
};

const KIT_PAGE: PageJump = { label: "Component kit", path: "/dev/kit", detail: "Every kit component with sample data" };

export function navGroups(extensions: readonly BootExtension[]): NavGroup[] {
  return [
    { title: null, items: [item("Home", "/", House, "g h")] },
    {
      title: "Investigate",
      items: [item("Requests", "/requests", List, "g r", ["/ips", "/keys"]), item("Traffic", "/traffic", Activity, "g t")],
    },
    { title: "People", items: [item("Accounts", "/accounts", Users, "g a"), item("Players", "/players", Box, "g p")] },
    { title: "Moderation", items: [item("Moderation", "/moderation", Shield, "g m")] },
    {
      title: "Platform",
      items: [
        item("Server", "/server", Server),
        item("Starfish", "/starfish", Star),
        item("Plugins", "/plugins", Plug),
        item("Guilds", "/guilds", Flag),
        ...extensions.map((extension) => ({ ...item(extension.title, `/x/${encodeURIComponent(extension.id)}`, Puzzle), badge: "ext" })),
      ],
    },
    { title: "Tools", items: [item("Batch resolve", "/tools/resolve", Wrench), item("Maintenance", "/maintenance", HardDrive)] },
  ];
}

export function pageJumps(groups: readonly NavGroup[]): PageJump[] {
  const pages = groups.flatMap((group) =>
    group.items.map((entry) => ({ label: entry.label, path: entry.path, detail: group.title === null ? "Page" : group.title })),
  );
  return [...pages, KIT_PAGE];
}

export function isNavActive(entry: NavItem, pathname: string): boolean {
  return [entry.path, ...entry.matches].some((path) => (path === "/" ? pathname === "/" : pathname === path || pathname.startsWith(`${path}/`)));
}

function item(label: string, path: string, icon: LucideIcon, shortcut: string | null = null, matches: readonly string[] = []): NavItem {
  return { label, path, icon, shortcut, matches, badge: null };
}
