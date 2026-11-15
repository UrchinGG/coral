export interface Page {
  title: string;
  path: string;
}

export interface Section {
  name: string;
  pages: Page[];
}

export const SECTIONS: Section[] = [
  {
    name: "Urchin",
    pages: [
      { title: "Home", path: "/urchin" },
      { title: "Members", path: "/urchin/members" },
      { title: "Players", path: "/urchin/players" },
      { title: "Guilds", path: "/urchin/guilds" },
      { title: "Tag Reviews", path: "/urchin/reviews" },
    ],
  },
  {
    name: "Starfish",
    pages: [
      { title: "Home", path: "/starfish" },
      { title: "Plugins", path: "/starfish/plugins" },
    ],
  },
  {
    name: "Sync",
    pages: [{ title: "Servers", path: "/sync/servers" }],
  },
  {
    name: "API",
    pages: [
      { title: "Traffic", path: "/platform/traffic" },
      { title: "Hypixel", path: "/platform/hypixel" },
      { title: "Maintenance & Audit", path: "/platform/maintenance" },
    ],
  },
];

export const sectionOf = (path: string) =>
  SECTIONS.find((section) => section.pages.some((page) => page.path === path));
