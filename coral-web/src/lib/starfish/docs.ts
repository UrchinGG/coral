const API_URL = process.env.CORAL_API_URL || "http://localhost:8000";
const REVALIDATE_SECONDS = 300;

export type DocEntry = { signature: string; description?: string };

export type DocTable = { columns?: [string, string, string]; rows: [string, string, string][] };

export type DocSection = {
  title: string;
  description?: string;
  entries?: DocEntry[];
  table?: DocTable;
};

export type DocPage = {
  slug: string;
  title: string;
  group: string;
  summary: string;
  sections: DocSection[];
  notes: string[];
};

export type StarfishDocs = {
  version: string;
  template: string;
  introduction: string;
  templateReference: string;
  groups: string[];
  sharedTypes: DocSection;
  pages: DocPage[];
};

export type NavSection = { label: string; items: { title: string; href: string }[] };

export async function starfishDocs(): Promise<StarfishDocs> {
  const res = await fetch(`${API_URL}/api/v1/starfish/docs`, { next: { revalidate: REVALIDATE_SECONDS } });
  if (!res.ok) throw new Error(`Starfish docs unavailable: ${res.status}`);
  return res.json();
}

export function docPage(docs: StarfishDocs, slug: string): DocPage | undefined {
  return docs.pages.find((page) => page.slug === slug);
}

export function docsNavigation(docs: StarfishDocs): NavSection[] {
  return docPagesByGroup(docs).map(({ group, pages }) => ({
    label: group,
    items: [
      ...(group === docs.groups[0] ? [{ title: "Introduction", href: "/docs" }] : []),
      ...pages.map((page) => ({ title: page.title, href: `/docs/${page.slug}` })),
    ],
  }));
}

export function docPagesByGroup(docs: StarfishDocs): { group: string; pages: DocPage[] }[] {
  return docs.groups.map((group) => ({ group, pages: docs.pages.filter((page) => page.group === group) }));
}
