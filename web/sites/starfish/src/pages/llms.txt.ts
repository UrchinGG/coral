import type { APIRoute } from "astro";
import { docs, type DocPage, type DocSection } from "@/lib/starfish";

export const GET: APIRoute = ({ site }) =>
  new Response(
    [
      "# Starfish Plugin API",
      "",
      "> Starfish is an injectable overlay and Lua plugin loader for Minecraft 1.8.9. This file is the complete Lua plugin API: every `starfish.*` function with its signature and the core events.",
      "",
      docs.introduction,
      "",
      "## Start here",
      "",
      docs.templateReference,
      "",
      "```lua",
      docs.template.trimEnd(),
      "```",
      "",
      sectionText(docs.sharedTypes, "##"),
      "## Pages",
      "",
      ...docs.pages.map(
        ({ slug, title, summary }) =>
          `- [${title}](${new URL(`/docs/${slug}`, site)}): ${summary}`,
      ),
      "",
      ...docs.pages.map(pageText),
    ].join("\n"),
  );

function pageText({ title, summary, sections, notes }: DocPage): string {
  return [
    `## ${title}`,
    "",
    summary,
    "",
    ...sections.map((section) => sectionText(section, "###")),
    ...(notes.length > 0
      ? ["### Notes", "", ...notes.map((note) => `- ${note}`), ""]
      : []),
  ].join("\n");
}

function sectionText(
  { title, description, entries = [], table }: DocSection,
  heading: string,
): string {
  return [
    `${heading} ${title}`,
    "",
    ...(description ? [description, ""] : []),
    ...entries.map(
      ({ signature, description }) =>
        `- \`${signature}\`${description ? `: ${description}` : ""}`,
    ),
    ...(table?.rows ?? []).map(
      ([name, type, description]) =>
        `- \`${name}\` (${type})${description ? `: ${description}` : ""}`,
    ),
    "",
  ].join("\n");
}
