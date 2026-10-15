import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";

import { blacklistQuery, tagTypesQuery } from "../../api/moderation";
import type { BlacklistRow, BlacklistSearchField, SortDirection, TagType } from "../../api/types/platform";
import { Badge } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { ClampText } from "../../kit/ClampText";
import { EntityLink } from "../../kit/EntityLink";
import { Input } from "../../kit/Input";
import { Segmented } from "../../kit/Segmented";
import { Select } from "../../kit/Select";
import type { Column } from "../../kit/Table";
import { useUrlParam } from "../../lib/url-state";
import { PagedTable } from "../platform/PagedTable";
import { When } from "../platform/When";
import { blacklistFiltersOf, DIRECTIONS, SEARCH_FIELDS } from "./params";
import { TagBadge } from "./TagBadge";

const FIELD_LABEL: Record<BlacklistSearchField, string> = { player: "Player", tagger: "Tagger", reason: "Reason" };
const FIELD_PLACEHOLDER: Record<BlacklistSearchField, string> = {
  player: "Name or UUID, either form",
  tagger: "Discord id of the tagger",
  reason: "Text in the reason",
};
const DIRECTION_LABEL: Record<SortDirection, string> = { desc: "Newest first", asc: "Oldest first" };
const ALL_TAGS = "";

export function BlacklistTab() {
  const [q, setQ] = useUrlParam("q");
  const [field, setField] = useUrlParam("field");
  const [tag, setTag] = useUrlParam("tag");
  const [dir, setDir] = useUrlParam("dir");
  const filters = blacklistFiltersOf(new URLSearchParams({ q: q ?? "", field: field ?? "", tag: tag ?? "", dir: dir ?? "" }));
  const types = useQuery(tagTypesQuery());
  const tags = useInfiniteQuery(blacklistQuery(filters));
  const [draft, setDraft] = useState(filters.q);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setQ(draft.trim());
  };

  return (
    <>
      <form onSubmit={submit} className="flex flex-wrap items-end gap-2">
        <Segmented label="Search by" value={filters.field} options={SEARCH_FIELDS.map((value) => ({ value, label: FIELD_LABEL[value] }))} onChange={(value) => setField(value === "player" ? null : value)} />
        <Input aria-label="Blacklist search" placeholder={FIELD_PLACEHOLDER[filters.field]} value={draft} onChange={(event) => setDraft(event.target.value)} className="w-72 max-w-full" />
        <Button type="submit">Search</Button>
        <Select aria-label="Tag" value={filters.tag} options={tagOptions(types.data)} onChange={(value) => setTag(value === ALL_TAGS ? null : value)} />
        <Select aria-label="Order" value={filters.dir} options={DIRECTIONS.map((value) => ({ value, label: DIRECTION_LABEL[value] }))} onChange={(value) => setDir(value === "desc" ? null : value)} />
      </form>
      <PagedTable
        title="Active tags"
        label="Active tags"
        query={tags}
        columns={columns(types.data)}
        rowKey={(row) => String(row.id)}
        empty={filters.q === "" && filters.tag === "" ? "No player carries an active tag." : "No active tag matches these filters."}
      />
    </>
  );
}

function tagOptions(types: readonly TagType[] | undefined) {
  return [{ value: ALL_TAGS, label: "Every tag" }, ...(types ?? []).map((type) => ({ value: type.name, label: type.display_name }))];
}

function columns(types: readonly TagType[] | undefined): Column<BlacklistRow>[] {
  return [
    { id: "player", header: "Player", cell: (row) => <EntityLink entity={row.player} /> },
    { id: "tag", header: "Tag", cell: (row) => <TagBadge tag={row.tag_type} types={types} /> },
    { id: "reason", header: "Reason", cell: (row) => (row.reason === null ? <span className="text-subtle">no reason</span> : <ClampText>{row.reason}</ClampText>), className: "min-w-[220px]" },
    { id: "by", header: "By", cell: (row) => <AuthorCell row={row} />, priority: 2 },
    { id: "at", header: "Added", cell: (row) => <When at={row.at} /> },
    { id: "expires", header: "Expires", cell: (row) => <When at={row.expires_at} empty="never" />, priority: 2 },
    { id: "lock", header: "Lock", cell: (row) => <LockCell row={row} />, priority: 2 },
  ];
}

function AuthorCell({ row }: { row: BlacklistRow }) {
  return (
    <span className="flex flex-wrap items-center gap-1.5">
      {row.author === null ? <span className="text-subtle">unknown</span> : <EntityLink entity={row.author} />}
      {row.hide_username && <Badge title="Public lookups show this tag without the tagger's name">hidden publicly</Badge>}
    </span>
  );
}

function LockCell({ row }: { row: BlacklistRow }) {
  if (row.lock === null) return <span className="text-subtle">—</span>;
  return (
    <div className="flex flex-col gap-0.5">
      <span className="flex flex-wrap items-center gap-1.5">
        <Badge variant="danger">Locked</Badge>
        {row.lock.by !== null && <EntityLink entity={row.lock.by} />}
        <When at={row.lock.at} />
      </span>
      {row.lock.reason !== null && <ClampText className="text-muted">{row.lock.reason}</ClampText>}
    </div>
  );
}
