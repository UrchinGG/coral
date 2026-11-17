import type { Permission } from "@/lib/session";

export interface Page {
  title: string;
  path: string;
  needs: Permission;
}

export interface Section {
  name: string;
  pages: Page[];
}

export const ACCOUNT: Page = {
  title: "Account",
  path: "/account",
  needs: "signed_in",
};

export const SITE = [
  ["Home", "/"],
  ["API Docs", "/docs"],
  ["Replays", "/replays"],
] as const;

export const SECTIONS: Section[] = [
  {
    name: "Urchin",
    pages: [
      { title: "Home", path: "/urchin", needs: "staff" },
      { title: "Members", path: "/urchin/members", needs: "members_read" },
      { title: "Players", path: "/urchin/players", needs: "players_read" },
      { title: "Guilds", path: "/urchin/guilds", needs: "players_read" },
      {
        title: "Tag Reviews",
        path: "/urchin/reviews",
        needs: "reviews_manage",
      },
      { title: "Shared Accounts", path: "/urchin/shared", needs: "staff" },
    ],
  },
  {
    name: "Starfish",
    pages: [
      { title: "Home", path: "/starfish", needs: "starfish" },
      { title: "Plugins", path: "/starfish/plugins", needs: "starfish" },
    ],
  },
  {
    name: "Sync",
    pages: [{ title: "Servers", path: "/sync/servers", needs: "owner" }],
  },
  {
    name: "API",
    pages: [
      { title: "Traffic", path: "/platform/traffic", needs: "traffic" },
      { title: "Hypixel", path: "/platform/hypixel", needs: "owner" },
      {
        title: "Maintenance & Audit",
        path: "/platform/maintenance",
        needs: "owner",
      },
    ],
  },
];

export const sectionOf = (path: string) =>
  SECTIONS.find((section) => section.pages.some((page) => page.path === path));
