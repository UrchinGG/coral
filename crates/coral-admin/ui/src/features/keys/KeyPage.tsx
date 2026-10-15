import { useInfiniteQuery, useQuery, type UseQueryResult } from "@tanstack/react-query";
import { KeyRound } from "lucide-react";
import { useMemo } from "react";
import { Navigate, useParams } from "react-router-dom";

import { keyByIdQuery, keyByPrefixQuery, keyRequestsQuery } from "../../api/apiKeys";
import type { KeyLookup, KeyMatch } from "../../api/types/apiKeys";
import { Badge, Pill } from "../../kit/Badges";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { CoverageBanner, SkeletonLines } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { EntityHeader, IdRow } from "../entity/EntityHeader";
import { RequestStream } from "../entity/RequestStream";
import { keyRouteOf } from "./key-route";

export function KeyPage() {
  const { "*": rest = "" } = useParams();
  const route = keyRouteOf(rest);
  return route.kind === "id" ? <KeyIdPage keyId={route.keyId} /> : <KeyPrefixPage prefix={route.prefix} />;
}

function KeyPrefixPage({ prefix }: { prefix: string }) {
  return <KeyLookupView lookup={useQuery(keyByPrefixQuery(prefix))} prefix={prefix} />;
}

function KeyIdPage({ keyId }: { keyId: number }) {
  const lookup = useQuery(keyByIdQuery(keyId));
  const current = lookup.data?.matches.find((match) => match.state === "current" && match.key !== undefined);
  if (current !== undefined) return <Navigate to={`/accounts/${current.account.discord_id}`} replace />;
  return <KeyLookupView lookup={lookup} prefix={lookup.data?.prefix ?? null} />;
}

function KeyLookupView({ lookup, prefix }: { lookup: UseQueryResult<KeyLookup, Error>; prefix: string | null }) {
  return (
    <>
      <QueryContent query={lookup} skeleton={<SkeletonLines widths={["30%", "50%"]} />}>
        {(data) => <KeyOverview lookup={data} />}
      </QueryContent>
      {prefix !== null && <PrefixStream prefix={prefix} />}
    </>
  );
}

function KeyOverview({ lookup }: { lookup: KeyLookup }) {
  const traffic = useTrafficMeta(lookup);
  const exact = lookup.matches.find((match) => match.key !== undefined);
  return (
    <>
      <EntityHeader
        kind="key"
        icon={<KeyRound className="size-5" />}
        title={<span className="font-mono">{lookup.prefix}</span>}
        badges={<PrefixBadge lookup={lookup} />}
        ids={
          exact?.key !== undefined && (
            <IdRow label="Key">
              <CopyField value={exact.key} label="Copy key" secret />
            </IdRow>
          )
        }
        meta={[plural(lookup.matches.length, "matching key"), ...traffic]}
      />
      <PrefixNotice lookup={lookup} />
      <Section title="Matching keys" count={lookup.matches.length} flush>
        <MatchTable matches={lookup.matches} />
      </Section>
    </>
  );
}

function PrefixBadge({ lookup }: { lookup: KeyLookup }) {
  if (lookup.internal) return <Badge variant="bot">internal key</Badge>;
  if (lookup.matches.length === 0) return <Pill tone="warning">unknown key</Pill>;
  return <Badge variant="key">key prefix</Badge>;
}

function PrefixNotice({ lookup }: { lookup: KeyLookup }) {
  if (lookup.internal) {
    return (
      <CoverageBanner>
        {lookup.prefix} is the prefix of the internal key that coral-bot, coral-sync and mc-verify send. Its requests belong to no account; bot requests name the Discord user they ran for as the actor.
      </CoverageBanner>
    );
  }
  if (lookup.matches.length > 0) return null;
  return <CoverageBanner tone="warning">No current, developer or retired key starts with {lookup.prefix}. Requests that presented it were rejected or counted against no account.</CoverageBanner>;
}

function useTrafficMeta(lookup: KeyLookup): string[] {
  const { time } = useTimeFormat();
  const { unattributed } = lookup;
  const kind = lookup.internal ? "internal-key" : "unattributed";
  const count = `${unattributed.capped ? "at least " : ""}${formatNumber(unattributed.requests_14d)} ${kind} requests in 14 d`;
  const last = unattributed.last === null ? `none ${kind}` : `last ${kind} request ${time(unattributed.last, { seconds: false, date: "always" })}`;
  return [count, last];
}

function MatchTable({ matches }: { matches: readonly KeyMatch[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<KeyMatch>[]>(
    () => [
      { id: "key", header: "Key", cell: (row) => (row.key === undefined ? <EntityLink entity={{ kind: "key", key_id: row.key_id, prefix: row.prefix, key_kind: row.kind === "developer" ? "developer" : "personal" }} /> : <CopyField value={row.key} label="Copy key" secret />) },
      { id: "owner", header: "Owner", cell: (row) => <EntityLink entity={row.account} /> },
      { id: "kind", header: "Kind", cell: (row) => <Badge variant="key">{row.kind}</Badge> },
      { id: "state", header: "State", cell: (row) => <KeyStatePill match={row} /> },
      { id: "created", header: "Created", cell: (row) => (row.created_at === null ? <span className="text-subtle">before history began</span> : time(row.created_at, { seconds: false, date: "always" })), priority: 2 },
      { id: "retired", header: "Retired", cell: (row) => (row.retired_at === null ? "—" : time(row.retired_at, { seconds: false, date: "always" })), priority: 2 },
    ],
    [time],
  );
  return <Table label="Keys with this prefix" columns={columns} rows={matches} rowKey={(row) => String(row.key_id)} empty="No key with this prefix belongs to an account." />;
}

function KeyStatePill({ match }: { match: KeyMatch }) {
  if (match.state === "retired") return <Pill>retired</Pill>;
  return match.locked ? <Pill tone="danger">current, locked</Pill> : <Pill tone="ok">current</Pill>;
}

function PrefixStream({ prefix }: { prefix: string }) {
  const requests = useInfiniteQuery(keyRequestsQuery(prefix));
  return (
    <Section title="Requests with this prefix" flush fetching={requests.isFetching && !requests.isFetchingNextPage && requests.data !== undefined}>
      <RequestStream label="Requests that presented this key prefix" query={requests} empty="No request presented this prefix in the last 14 days." />
    </Section>
  );
}
