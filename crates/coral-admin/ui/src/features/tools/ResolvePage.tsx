import { useMutation } from "@tanstack/react-query";

import { MAX_RESOLVE_INPUTS, resolveIdentifiers } from "../../api/tools";
import type { Resolution, ResolveKind } from "../../api/types/platform";
import { Button } from "../../kit/Button";
import { EntityLink } from "../../kit/EntityLink";
import { Textarea } from "../../kit/Input";
import { Section } from "../../kit/Section";
import { ErrorState } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { plural } from "../../lib/format";
import { useHistoryValue } from "../../lib/history-state";
import { PageHead } from "../platform/PageHead";

const INPUT_STATE_KEY = "tools.resolve";
const INPUT_ROWS = 8;
const LINE_BREAK = /\r?\n/;

const KIND_LABEL: Record<ResolveKind, string> = { discord: "Discord id", player: "Player UUID", name: "Player name", ip: "IP", unknown: "Unrecognised" };

const COLUMNS: readonly Column<Resolution>[] = [
  { id: "input", header: "Input", cell: (row) => <span className="font-mono text-12 wrap-anywhere">{row.input}</span> },
  { id: "kind", header: "Kind", cell: (row) => KIND_LABEL[row.kind] },
  { id: "entity", header: "Resolves to", cell: (row) => (row.entity === null ? <span className="text-subtle">nothing</span> : <EntityLink entity={row.entity} />) },
  { id: "note", header: "Note", cell: (row) => <span className="text-muted">{row.note}</span>, priority: 2 },
];

export function ResolvePage() {
  const [text, setText] = useHistoryValue(INPUT_STATE_KEY);
  const resolve = useMutation({ mutationFn: resolveIdentifiers });
  const inputs = linesOf(text);
  const tooMany = inputs.length > MAX_RESOLVE_INPUTS;

  return (
    <>
      <PageHead title="Batch resolve" lede="Paste names, UUIDs in either form, Discord ids or IPs, one per line, and resolve them together from the caches." />
      <Section title="Identifiers" count={plural(inputs.length, "distinct line")}>
        <div className="flex flex-col gap-2">
          <Textarea aria-label="Identifiers, one per line" mono rows={INPUT_ROWS} value={text} onChange={(event) => setText(event.target.value)} error={tooMany ? `Resolve at most ${MAX_RESOLVE_INPUTS} lines at once.` : undefined} />
          <span>
            <Button variant="primary" disabled={inputs.length === 0 || tooMany} pending={resolve.isPending} onClick={() => resolve.mutate(inputs)}>
              Resolve
            </Button>
          </span>
        </div>
      </Section>
      {resolve.error !== null && <ErrorState error={resolve.error} onRetry={() => resolve.mutate(inputs)} retrying={resolve.isPending} />}
      {resolve.data !== undefined && (
        <Section title="Results" count={resolve.data.results.length} flush>
          <Table label="Resolved identifiers" columns={COLUMNS} rows={resolve.data.results} rowKey={(row) => row.input} empty="No identifier to resolve." />
        </Section>
      )}
    </>
  );
}

function linesOf(text: string): string[] {
  const lines = text
    .split(LINE_BREAK)
    .map((line) => line.trim())
    .filter((line) => line !== "");
  return [...new Set(lines)];
}
