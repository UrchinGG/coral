import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { remoteSearchQuery, searchQuery } from "../../api/search";
import type { SearchHit, SearchResponse } from "../../api/types/search";
import { EntityLink } from "../../kit/EntityLink";
import { Input } from "../../kit/Input";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { CoverageBanner } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { useDebouncedValue } from "../../lib/debounce";
import { useHistoryValue } from "../../lib/history-state";
import { ENTITY_KIND_LABEL } from "../../lib/refs";
import { classify, describeClassification } from "../../omnibox/classify";
import { failureSummary, hintItems, hitKey, hitPath, SEARCH_STATE_KEY, searchActions, type OmniboxItem } from "../../omnibox/results";

const SEARCH_PAGE_LIMIT = 50;
const SEARCH_PAGE_DEBOUNCE_MS = 200;

const COLUMNS: readonly Column<SearchHit>[] = [
  { id: "entity", header: "Match", cell: (hit) => <EntityLink entity={hit.ref} tab={hit.tab} /> },
  { id: "kind", header: "Kind", cell: (hit) => ENTITY_KIND_LABEL[hit.ref.kind] },
  { id: "account", header: "Account", cell: (hit) => <OwningAccount hit={hit} />, priority: 2 },
  { id: "reason", header: "Why", cell: (hit) => <span className="text-muted">{hit.reason}</span>, priority: 2 },
  { id: "score", header: "Score", cell: (hit) => hit.score, numeric: true, priority: 2 },
];

export function SearchPage() {
  const [text, setText] = useHistoryValue(SEARCH_STATE_KEY);
  const settled = useDebouncedValue(text.trim(), SEARCH_PAGE_DEBOUNCE_MS);
  const search = useQuery({ ...searchQuery(settled, SEARCH_PAGE_LIMIT), placeholderData: keepPreviousData });
  const classification = classify(settled, window.location.origin);
  const remotePending = search.data?.remote_pending === true;
  const remote = useQuery({ ...remoteSearchQuery(classification.text, remotePending && !search.isPlaceholderData), placeholderData: keepPreviousData });
  const shapeLinks = hintItems(classification);
  const actions = searchActions(classification).filter(leavesThisPage);

  return (
    <>
      <div className="flex flex-col gap-1">
        <h1 className="text-22 font-semibold">Search</h1>
        <p className="m-0 text-muted">Every interpretation of the input, ranked. The text stays in this tab's history, never in the address bar.</p>
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="w-[min(520px,100%)]">
          <Input label="Search for" value={text} onChange={(event) => setText(event.target.value)} className="w-full" />
        </div>
        {settled !== "" && <span className="pb-1.5 text-12 text-subtle">{describeClassification(classification)}</span>}
      </div>
      {settled === "" ? (
        <p className="m-0 text-muted">Type a player, UUID, Discord id or name, IP, key prefix or request id.</p>
      ) : (
        <>
          <Section title="Results" count={search.data === undefined ? undefined : search.data.hits.length} flush fetching={search.isFetching && search.data !== undefined}>
            <QueryContent query={search}>
              {(data) => <SearchResults data={data} />}
            </QueryContent>
          </Section>
          {remotePending && (
            <Section title="From Mojang" count={remote.data?.length} flush fetching={remote.data !== undefined && (remote.isFetching || remote.isPlaceholderData)}>
              <QueryContent query={remote} isEmpty={(hits) => hits.length === 0} empty={`Mojang knows no player named "${classification.text}".`}>
                {(hits) => <Table label="Mojang results" columns={COLUMNS} rows={hits} rowKey={hitKey} />}
              </QueryContent>
            </Section>
          )}
        </>
      )}
      {search.error !== null && shapeLinks.length > 0 && <LinkList title="From the shape of the input" items={shapeLinks} />}
      {actions.length > 0 && <LinkList title="More" items={actions} />}
    </>
  );
}

function SearchResults({ data }: { data: SearchResponse }) {
  const failures = failureSummary(data.failures);
  const empty = failures === null ? "Nothing matches." : "Nothing matches among the lookups that answered.";
  return (
    <div className="flex flex-col gap-2">
      {data.interpretations.length > 0 && <p className="m-0 px-3.5 pt-2.5 text-12 text-subtle">Read as {data.interpretations.join(" · ")}</p>}
      {failures !== null && (
        <div className="px-3.5">
          <CoverageBanner tone="warning">{failures}</CoverageBanner>
        </div>
      )}
      <Table label="Search results" columns={COLUMNS} rows={data.hits} rowKey={hitKey} empty={empty} />
      {data.hits.length >= SEARCH_PAGE_LIMIT && <p className="m-0 px-3.5 pb-2.5 text-12 text-subtle">The best {SEARCH_PAGE_LIMIT} matches. Narrow the text or force a type to reach the rest.</p>}
    </div>
  );
}

function leavesThisPage(item: OmniboxItem): boolean {
  return item.type === "link" && item.path !== "/search";
}

function OwningAccount({ hit }: { hit: SearchHit }) {
  const ownAccount = hit.ref.kind === "account" && hit.ref.discord_id === hit.account_id;
  if (hit.account_id === null || ownAccount) return <span className="text-subtle">—</span>;
  return <EntityLink entity={{ kind: "account", discord_id: hit.account_id, member_id: null }} />;
}

function LinkList({ title, items }: { title: string; items: readonly OmniboxItem[] }) {
  return (
    <Section title={title}>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-12">
        {items.map((item) => (
          <li key={item.key}>
            {item.type === "entity" ? (
              <EntityLink entity={item.ref} tab={item.tab} />
            ) : (
              <Link to={item.path} state={item.state ?? undefined} className="text-accent hover:underline">
                {item.label}
              </Link>
            )}
            <span className="ml-2 text-muted">{item.type === "entity" ? hitPath(item) : item.detail}</span>
          </li>
        ))}
      </ul>
    </Section>
  );
}
