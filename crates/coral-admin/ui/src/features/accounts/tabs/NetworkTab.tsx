import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { accountNetworkQuery } from "../../../api/accounts";
import type { AccountCore, AccountNetwork, IpUse } from "../../../api/types/accounts";
import { Badge } from "../../../kit/Badges";
import { EntityLink } from "../../../kit/EntityLink";
import { Meter } from "../../../kit/Meter";
import { QueryContent } from "../../../kit/QueryContent";
import { Section } from "../../../kit/Section";
import { EmptyState } from "../../../kit/States";
import { Table, type Column } from "../../../kit/Table";
import { formatNumber, plural } from "../../../lib/format";
import { useTimeFormat } from "../../../lib/time";
import { AgentTable } from "../../entity/AgentTable";

type Country = AccountNetwork["countries"][number] & { total: number };

export function NetworkTab({ core }: { core: AccountCore }) {
  const network = useQuery(accountNetworkQuery(core.account.discord_id));
  const fetching = network.isFetching && network.data !== undefined;
  if (core.member === null) return <EmptyState>Only Coral accounts have recorded IPs and user agents; bot commands run from the bot's own address.</EmptyState>;
  return (
    <QueryContent query={network}>
      {(data) => (
        <>
          <Section title="IP addresses" count={data.ips.length} flush fetching={fetching}>
            <IpTable ips={data.ips} />
          </Section>
          <Section title="User agents" count={data.agents.length} flush fetching={fetching}>
            <AgentTable agents={data.agents} label="User agents of this account" />
          </Section>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Section title="Countries" flush fetching={fetching}>
              <CountryTable countries={data.countries} />
            </Section>
            <Section title="Linked accounts" count={data.linked_accounts.length} flush fetching={fetching}>
              <LinkedAccounts linked={data.linked_accounts} />
            </Section>
          </div>
        </>
      )}
    </QueryContent>
  );
}

function IpTable({ ips }: { ips: readonly IpUse[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<IpUse>[]>(
    () => [
      { id: "ip", header: "IP", cell: (row) => <EntityLink entity={row.ip} /> },
      { id: "requests", header: "Requests", cell: (row) => (row.imported ? <Badge title="Imported on 2026-03-26 without request counts">imported</Badge> : formatNumber(row.requests)), numeric: true },
      { id: "first", header: "First", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.first_seen, { seconds: false, date: "always" })}</span>, priority: 2 },
      { id: "last", header: "Last", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.last_seen, { seconds: false, date: "always" })}</span> },
      { id: "country", header: "Country", cell: (row) => <span className="font-mono">{row.ip.country ?? "—"}</span>, priority: 2 },
      { id: "shared", header: "Shared with", cell: (row) => <SharedWith ip={row} /> },
    ],
    [time],
  );
  return <Table label="IP addresses of this account" columns={columns} rows={ips} rowKey={(row) => row.ip.ip} height="420px" empty="No IP recorded for this account." />;
}

function SharedWith({ ip }: { ip: IpUse }) {
  if (ip.shared_count === 0) return <span className="text-subtle">only this account</span>;
  const hidden = ip.shared_count - ip.shared_with.length;
  return (
    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
      <EntityLink entity={ip.ip} label={plural(ip.shared_count, "account")} plain />
      {ip.shared_with.map((account) => (
        <EntityLink key={account.discord_id} entity={account} />
      ))}
      {hidden > 0 && <span className="text-11 text-subtle">and {formatNumber(hidden)} more on the IP page</span>}
    </span>
  );
}

function CountryTable({ countries }: { countries: AccountNetwork["countries"] }) {
  const total = countries.reduce((sum, country) => sum + country.requests, 0);
  const rows: Country[] = countries.map((country) => ({ ...country, total }));
  return (
    <Table
      label="Countries of this account"
      columns={COUNTRY_COLUMNS}
      rows={rows}
      rowKey={(row) => row.country ?? "unknown"}
      empty="No country recorded."
    />
  );
}

const COUNTRY_COLUMNS: readonly Column<Country>[] = [
  { id: "country", header: "Country", cell: (row) => <span className="font-mono">{row.country ?? "unknown"}</span> },
  { id: "requests", header: "Requests", cell: (row) => formatNumber(row.requests), numeric: true },
  { id: "share", header: "Share", cell: (row) => <Meter used={row.requests} limit={Math.max(1, row.total)} inline /> },
];

function LinkedAccounts({ linked }: { linked: AccountNetwork["linked_accounts"] }) {
  if (linked.length === 0) return <EmptyState>No other account shares an IP with this one.</EmptyState>;
  return (
    <ul className="m-0 flex list-none flex-col p-0">
      {linked.map((entry) => (
        <li key={entry.account.discord_id} className="flex flex-wrap items-baseline gap-x-2 gap-y-1 border-b border-border px-3.5 py-[7px] text-12 last:border-b-0">
          <EntityLink entity={entry.account} />
          <span className="text-muted">shares</span>
          {entry.ips.map((ip) => (
            <EntityLink key={ip.ip} entity={ip} />
          ))}
        </li>
      ))}
    </ul>
  );
}
