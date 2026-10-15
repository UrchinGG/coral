import { useInfiniteQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { starfishUsersQuery } from "../../api/starfish";
import type { StarfishUser } from "../../api/types/platform";
import { Badge, Pill } from "../../kit/Badges";
import { EntityLink } from "../../kit/EntityLink";
import type { Column } from "../../kit/Table";
import type { Tone } from "../../kit/tone";
import { formatNumber } from "../../lib/format";
import { refHref } from "../../lib/refs";
import { useUrlParam } from "../../lib/url-state";
import { PageHead } from "../platform/PageHead";
import { PagedTable } from "../platform/PagedTable";
import { SearchBox } from "../platform/SearchBox";
import { When } from "../platform/When";

const ACCOUNT_TAB = "starfish";
const LICENSE_TONE: Record<string, Tone> = { active: "ok", suspended: "danger", inactive: "neutral" };

const COLUMNS: readonly Column<StarfishUser>[] = [
  { id: "account", header: "Account", cell: (user) => <EntityLink entity={user.account} tab={ACCOUNT_TAB} /> },
  { id: "license", header: "License", cell: (user) => <Pill tone={LICENSE_TONE[user.license_status] ?? "neutral"}>{user.license_status}</Pill> },
  { id: "session", header: "Session", cell: (user) => (user.active_session ? <Badge>active</Badge> : <span className="text-subtle">—</span>) },
  { id: "hwids", header: "HWIDs", cell: (user) => formatNumber(user.hwids), numeric: true },
  { id: "plugins", header: "Plugins", cell: (user) => <PluginCounts user={user} />, priority: 2 },
  { id: "github", header: "GitHub", cell: (user) => (user.github_username === null ? <span className="text-subtle">—</span> : <span className="font-mono text-12">{user.github_username}</span>), priority: 2 },
  { id: "heartbeat", header: "Last heartbeat", cell: (user) => <When at={user.last_heartbeat_at} /> },
];

export function StarfishPage() {
  const [q, setQ] = useUrlParam("q");
  const users = useInfiniteQuery(starfishUsersQuery(q ?? ""));
  const navigate = useNavigate();
  return (
    <>
      <PageHead title="Starfish" lede="Licensed Starfish users. Each user's licence, hardware ids, sessions and plugins are on the Starfish tab of their account." />
      <SearchBox label="Search Starfish users" placeholder="Discord id, Discord name or GitHub name" value={q ?? ""} onSearch={setQ} />
      <PagedTable
        title="Starfish users"
        label="Starfish users"
        query={users}
        columns={COLUMNS}
        rowKey={(user) => String(user.user_id)}
        onRowClick={(user) => navigate(`${refHref(user.account)}?tab=${ACCOUNT_TAB}`)}
        empty={q === null ? "No user holds an active or suspended licence." : `No licensed user matches "${q}".`}
      />
    </>
  );
}

function PluginCounts({ user }: { user: StarfishUser }) {
  return (
    <span>
      {formatNumber(user.plugins_installed)} installed
      {user.plugins_owned > 0 && <span className="text-subtle"> · {formatNumber(user.plugins_owned)} published</span>}
    </span>
  );
}
