import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { Globe } from "lucide-react";
import { useMemo } from "react";
import { useParams } from "react-router-dom";

import { ipCoreQuery, ipRequestsQuery } from "../../api/ips";
import type { IpAccount, IpCore } from "../../api/types/ips";
import { Badge } from "../../kit/Badges";
import { CopyField } from "../../kit/Copy";
import { EntityLink } from "../../kit/EntityLink";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { CoverageBanner, SkeletonLines } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { AgentTable } from "../entity/AgentTable";
import { EntityHeader } from "../entity/EntityHeader";
import { RequestStream } from "../entity/RequestStream";

const SHARED_NAMED = 8;

export function IpPage() {
  const { ip = "" } = useParams();
  const core = useQuery(ipCoreQuery(ip));
  const requests = useInfiniteQuery(ipRequestsQuery(ip));
  return (
    <>
      <QueryContent query={core} skeleton={<SkeletonLines widths={["30%", "50%"]} />}>
        {(data) => <IpOverview ip={data} />}
      </QueryContent>
      <Section title="Request stream" flush fetching={requests.isFetching && !requests.isFetchingNextPage && requests.data !== undefined}>
        <RequestStream label="Requests from this IP" query={requests} empty="No request from this IP in the last 14 days." />
      </Section>
    </>
  );
}

function IpOverview({ ip }: { ip: IpCore }) {
  return (
    <>
      <EntityHeader
        icon={<Globe className="size-5" />}
        kind="ip"
        title={<span className="font-mono">{ip.ip}</span>}
        badges={ip.proxied && <Badge variant="cf" title="Requests arrived through a Cloudflare Worker; this is the Worker's outbound address">CF Worker</Badge>}
        ids={<CopyField value={ip.ip} label="Copy IP" />}
        meta={[`Country ${ip.country ?? "unknown"}`, plural(ip.accounts.length, "account"), `${ip.requests_24h_capped ? "at least " : ""}${formatNumber(ip.requests_24h)} requests in 24 h`]}
      />
      {ip.accounts.length > 1 && (
        <CoverageBanner tone="warning">
          <span className="inline-flex flex-wrap items-center gap-1.5">
            {formatNumber(ip.accounts.length)} accounts share this address:
            {ip.accounts.slice(0, SHARED_NAMED).map((account) => (
              <EntityLink key={account.account.discord_id} entity={account.account} />
            ))}
            {ip.accounts.length > SHARED_NAMED && <span>and {formatNumber(ip.accounts.length - SHARED_NAMED)} more in the table below</span>}
          </span>
        </CoverageBanner>
      )}
      <Section title="Accounts" count={ip.accounts.length} flush>
        <AccountTable accounts={ip.accounts} />
      </Section>
      <Section title="User agents" count={`newest ${formatNumber(ip.agents_sample)} requests`} flush>
        <AgentTable agents={ip.agents} label="User agents seen on this IP" />
      </Section>
    </>
  );
}

function AccountTable({ accounts }: { accounts: readonly IpAccount[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<IpAccount>[]>(
    () => [
      { id: "account", header: "Account", cell: (row) => <EntityLink entity={row.account} /> },
      { id: "requests", header: "Requests", cell: (row) => (row.imported ? <Badge title="Imported on 2026-03-26 without request counts">imported</Badge> : formatNumber(row.requests)), numeric: true },
      { id: "first", header: "First", cell: (row) => time(row.first_seen, { seconds: false, date: "always" }), priority: 2 },
      { id: "last", header: "Last", cell: (row) => time(row.last_seen, { seconds: false, date: "always" }) },
    ],
    [time],
  );
  return (
    <Table
      label="Accounts seen on this IP"
      columns={columns}
      rows={accounts}
      rowKey={(row) => row.account.discord_id}
      height="420px"
      empty="No Coral account has used this address; only unauthenticated or bot requests came from it."
    />
  );
}
