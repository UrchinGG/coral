import {
  DOC_PAGES,
  INTRODUCTION,
  SHARED_TYPES,
  TEMPLATE_PLUGIN,
  TEMPLATE_REFERENCE,
  type DocPage,
  type DocSection,
} from "@/lib/starfish/docs";

export function llmsTxt(origin: string): string {
  return [
    "# Starfish Plugin API",
    "",
    "> Starfish is an injectable overlay and Lua plugin loader for Minecraft 1.8.9. This file is the complete Lua plugin API: every `starfish.*` function with its signature and the core events.",
    "",
    INTRODUCTION,
    "",
    "## Start here",
    "",
    TEMPLATE_REFERENCE,
    "",
    luaBlock(TEMPLATE_PLUGIN.trimEnd()),
    "",
    sectionText(SHARED_TYPES, "##"),
    "## Pages",
    "",
    ...DOC_PAGES.map((page) => `- [${page.title}](${origin}/docs/${page.slug}): ${page.summary}`),
    "",
    ...DOC_PAGES.map(pageText),
  ].join("\n");
}

function pageText(page: DocPage): string {
  return [
    `## ${page.title}`,
    "",
    page.summary,
    "",
    ...page.sections.map((section) => sectionText(section, "###")),
    ...(page.notes.length > 0 ? ["### Notes", "", ...page.notes.map((note) => `- ${note}`), ""] : []),
  ].join("\n");
}

function sectionText(section: DocSection, heading: string): string {
  return [
    `${heading} ${section.title}`,
    "",
    ...(section.description ? [section.description, ""] : []),
    ...(section.entries ?? []).map(({ signature, description }) => `- \`${signature}\`${description ? `: ${description}` : ""}`),
    ...(section.table?.rows ?? []).map(([name, type, description]) => `- \`${name}\` (${type})${description ? `: ${description}` : ""}`),
    "",
  ].join("\n");
}

function luaBlock(code: string): string {
  return ["```lua", code, "```"].join("\n");
}
