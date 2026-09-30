import Link from "next/link";
import type { Metadata } from "next";
import { Code, P, Prose, Section } from "@/components/starfish/DocsKit";
import { DocSectionView } from "@/components/starfish/DocPage";
import {
  INTRODUCTION,
  SHARED_TYPES,
  TEMPLATE_PLUGIN,
  TEMPLATE_REFERENCE,
  docPagesByGroup,
} from "@/lib/starfish/docs";

export const metadata: Metadata = {
  title: "Plugin API",
  description: "Lua plugin API reference for Starfish.",
};

export default function DocsPage() {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">Plugin API</h1>
      <p className="text-sm text-white/40 mb-10">Lua scripting reference for Starfish plugins.</p>

      <Section title="Start here">
        <P>
          <Prose text={INTRODUCTION} />
        </P>
        <P>
          <Prose text={TEMPLATE_REFERENCE} />
        </P>
        <Code>{TEMPLATE_PLUGIN}</Code>
      </Section>

      <DocSectionView section={SHARED_TYPES} />

      <Section title="Reference">
        <div className="space-y-6">
          {docPagesByGroup().map(({ group, pages }) => (
            <div key={group}>
              <h3 className="text-[11px] text-white/20 uppercase tracking-widest mb-2">{group}</h3>
              <div className="space-y-3">
                {pages.map((page) => (
                  <DocLink key={page.slug} href={`/docs/${page.slug}`} title={page.title} description={page.summary} />
                ))}
              </div>
            </div>
          ))}
        </div>
      </Section>

      <Section title="For AI agents">
        <P>
          The whole API as one plain-text file, for pasting into an LLM:{" "}
          <a href="/llms.txt" className="text-white/70 underline underline-offset-2 hover:text-white">/llms.txt</a>
        </P>
      </Section>
    </div>
  );
}

function DocLink({ href, title, description }: { href: string; title: string; description: string }) {
  return (
    <Link href={href} className="block rounded-lg border border-white/[0.08] bg-[rgba(0,0,0,0.5)] p-4 hover:border-white/[0.14] transition-colors">
      <h3 className="text-sm font-semibold text-white/70 mb-1">{title}</h3>
      <p className="text-[12px] text-white/35">{description}</p>
    </Link>
  );
}
