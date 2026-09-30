import type { DocPage, DocSection } from "@/lib/starfish/docs";
import { P, PropTable, Prose, Section, Signature } from "@/components/starfish/DocsKit";

export function DocPageView({ page }: { page: DocPage }) {
  return (
    <div>
      <h1 className="text-3xl font-bold tracking-tight mb-2">{page.title}</h1>
      <p className="text-sm text-white/40 mb-10">{page.summary}</p>

      {page.sections.map((section) => (
        <DocSectionView key={section.title} section={section} />
      ))}

      {page.notes.length > 0 && (
        <Section title="Notes">
          <ul className="list-disc pl-5 space-y-2 text-sm text-white/50 leading-relaxed">
            {page.notes.map((note) => (
              <li key={note}>
                <Prose text={note} />
              </li>
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}

export function DocSectionView({ section }: { section: DocSection }) {
  return (
    <Section title={section.title}>
      {section.description && (
        <P>
          <Prose text={section.description} />
        </P>
      )}
      {section.entries?.map((entry) => (
        <div key={entry.signature}>
          <Signature>{entry.signature}</Signature>
          {entry.description && (
            <P>
              <Prose text={entry.description} />
            </P>
          )}
        </div>
      ))}
      {section.table && <PropTable columns={section.table.columns} rows={section.table.rows} />}
    </Section>
  );
}
