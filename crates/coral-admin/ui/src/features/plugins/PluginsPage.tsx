import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { pluginsQuery } from "../../api/plugins";
import type { PluginRow } from "../../api/types/platform";
import { EntityLink } from "../../kit/EntityLink";
import type { Column } from "../../kit/Table";
import { formatDecimal, formatNumber } from "../../lib/format";
import { refHref } from "../../lib/refs";
import { useUrlParam } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { PagedTable } from "../platform/PagedTable";
import { SearchBox } from "../platform/SearchBox";
import { When } from "../platform/When";
import { PluginFlags } from "./PluginFlags";

const COLUMNS: readonly Column<PluginRow>[] = [
  { id: "plugin", header: "Plugin", cell: (row) => <PluginCell row={row} /> },
  { id: "flags", header: "Status", cell: (row) => <PluginFlags official={row.official} unlisted={row.unlisted} disabled={row.disabled} /> },
  { id: "owner", header: "Owner", cell: (row) => (row.owner === null ? <span className="text-subtle">{row.owner_github ?? "unknown"}</span> : <EntityLink entity={row.owner} />), priority: 2 },
  { id: "latest", header: "Latest", cell: (row) => (row.latest_version === null ? <span className="text-subtle">no live release</span> : <span className="font-mono text-12">{row.latest_version}</span>) },
  { id: "installs", header: "Installs 30 d / total", cell: (row) => `${formatNumber(row.installs_30d)} / ${formatNumber(row.installs_total)}`, numeric: true },
  { id: "rating", header: "Rating", cell: (row) => (row.rating_mean === null ? <span className="text-subtle">—</span> : `${formatDecimal(row.rating_mean)} (${formatNumber(row.rating_count)})`), numeric: true, priority: 2 },
  { id: "created", header: "Created", cell: (row) => <When at={row.created_at} />, priority: 2 },
];

export function PluginsPage() {
  const [q, setQ] = useUrlParam("q");
  const plugins = useInfiniteQuery(pluginsQuery(q ?? ""));
  const navigate = useNavigate();
  return (
    <>
      <PageHead title="Plugins" lede="The Starfish plugin registry, including unlisted and disabled plugins and those whose releases are all yanked." />
      <SearchBox label="Search plugins" placeholder="Slug, name or description" value={q ?? ""} onSearch={setQ} />
      <PagedTable
        title="Plugins"
        label="Plugins"
        query={plugins}
        columns={COLUMNS}
        rowKey={(row) => row.plugin.slug}
        onRowClick={(row) => {
          const href = refHref(row.plugin);
          if (href !== null) navigate(href);
        }}
        empty={q === null ? "No plugin has been published." : `No plugin matches "${q}".`}
      />
    </>
  );
}

function PluginCell({ row }: { row: PluginRow }) {
  return (
    <div className="flex min-w-0 flex-col">
      <EntityLink entity={row.plugin} />
      <span className="font-mono text-11 text-subtle">{row.plugin.slug}</span>
    </div>
  );
}
