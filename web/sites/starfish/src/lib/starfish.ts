import { CORAL_API_URL } from "astro:env/server";

export type DocSection = {
  title: string;
  description?: string;
  entries?: { signature: string; description?: string }[];
  table?: {
    columns?: [string, string, string];
    rows: [string, string, string][];
  };
};

export type DocPage = {
  slug: string;
  title: string;
  group: string;
  summary: string;
  sections: DocSection[];
  notes: string[];
};

type StarfishDocs = {
  template: string;
  introduction: string;
  templateReference: string;
  groups: string[];
  sharedTypes: DocSection;
  pages: DocPage[];
};

type Release = {
  published_at: string;
  release_notes: string | null;
};

async function fetchStarfish<T>(path: string): Promise<T> {
  const response = await fetch(`${CORAL_API_URL}/api/v1/starfish/${path}`);
  if (!response.ok) {
    throw new Error(`Starfish ${path} unavailable: ${response.status}`);
  }
  return response.json();
}

export const [docs, releases] = await Promise.all([
  fetchStarfish<StarfishDocs>("docs"),
  fetchStarfish<Release[]>("download/releases"),
]);

export const docGroups = docs.groups.map((group) => ({
  group,
  pages: docs.pages.filter((page) => page.group === group),
}));
