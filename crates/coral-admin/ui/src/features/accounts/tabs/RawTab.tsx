import type { AccountCore } from "../../../api/types/accounts";
import { CopyButton } from "../../../kit/Copy";
import { LazyJsonView } from "../../../kit/LazyJsonView";
import { Section } from "../../../kit/Section";
import { EmptyState } from "../../../kit/States";

const JSON_INDENT = 2;
const ENTITY_LINKS = { players: true, accounts: true } as const;

export function RawTab({ core }: { core: AccountCore }) {
  if (core.member === null) return <EmptyState>No member row: this Discord user has no Coral account.</EmptyState>;
  const { config, row } = core.member;
  return (
    <>
      <Section title="config" actions={<CopyButton text={() => JSON.stringify(config, null, JSON_INDENT)} label="Copy config" />}>
        <LazyJsonView value={config} label="Member config" links={ENTITY_LINKS} />
      </Section>
      <Section title="Member row" actions={<CopyButton text={() => JSON.stringify(row, null, JSON_INDENT)} label="Copy member row" />}>
        <LazyJsonView value={row} label="Member row" links={ENTITY_LINKS} />
      </Section>
    </>
  );
}
