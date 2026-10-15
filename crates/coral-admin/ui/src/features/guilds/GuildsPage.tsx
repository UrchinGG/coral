import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { guildsQuery } from "../../api/guilds";
import type { GuildRow, GuildSort } from "../../api/types/platform";
import { EntityLink } from "../../kit/EntityLink";
import { Select } from "../../kit/Select";
import type { Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { refHref } from "../../lib/refs";
import { useUrlEnum, useUrlParam } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { PagedTable } from "../platform/PagedTable";
import { SearchBox } from "../platform/SearchBox";
import { When } from "../platform/When";
import { GUILD_SORTS } from "./params";

const SORT_LABEL: Record<GuildSort, string> = { updated: "Recently refreshed", members: "Most members", level: "Highest level", experience: "Most experience" };

const COLUMNS: readonly Column<GuildRow>[] = [
  { id: "guild", header: "Guild", cell: (row) => <GuildCell row={row} /> },
  { id: "tag", header: "Tag", cell: (row) => (row.tag === null ? <span className="text-subtle">—</span> : <span className="font-mono text-12">[{row.tag}]</span>) },
  { id: "level", header: "Level", cell: (row) => formatNumber(row.level), numeric: true },
  { id: "members", header: "Members", cell: (row) => formatNumber(row.member_count), numeric: true },
  { id: "experience", header: "Experience", cell: (row) => formatNumber(row.experience), numeric: true, priority: 2 },
  { id: "updated", header: "Refreshed", cell: (row) => <When at={row.updated_at} /> },
];

export function GuildsPage() {
  const [q, setQ] = useUrlParam("q");
  const [sort, setSort] = useUrlEnum("sort", GUILD_SORTS, "updated");
  const guilds = useInfiniteQuery(guildsQuery(q ?? "", sort));
  const navigate = useNavigate();
  return (
    <>
      <PageHead title="Guilds" lede="Hypixel guilds in the snapshot cache, with each guild's full snapshot history." />
      <div className="flex flex-wrap items-end gap-2">
        <SearchBox label="Search guilds" placeholder="Name, tag or guild id" value={q ?? ""} onSearch={setQ} />
        <Select aria-label="Sort" value={sort} options={GUILD_SORTS.map((value) => ({ value, label: SORT_LABEL[value] }))} onChange={setSort} />
      </div>
      <PagedTable
        title="Cached guilds"
        label="Cached guilds"
        query={guilds}
        columns={COLUMNS}
        rowKey={(row) => row.guild.guild_id}
        onRowClick={(row) => {
          const href = refHref(row.guild);
          if (href !== null) navigate(href);
        }}
        empty={q === null ? "No guild has been cached." : `No cached guild matches "${q}".`}
      />
    </>
  );
}

function GuildCell({ row }: { row: GuildRow }) {
  return (
    <div className="flex min-w-0 flex-col">
      <EntityLink entity={row.guild} />
      <span className="font-mono text-11 text-subtle">{row.guild.guild_id}</span>
    </div>
  );
}
