import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import { lookupStreamQuery, lookupSummaryQuery, topLookupsQuery } from "../../../api/accounts";
import type { AccountCore, LookupRow, LookupSummary, LookupVia, LookupWindow, TopLookups } from "../../../api/types/accounts";
import { useBoot } from "../../../app/boot";
import { Badge } from "../../../kit/Badges";
import { CoverageNotices } from "../../../kit/CoverageNotices";
import { EntityLink } from "../../../kit/EntityLink";
import { KpiStrip, KpiTile } from "../../../kit/KpiTile";
import { QueryContent } from "../../../kit/QueryContent";
import { Section } from "../../../kit/Section";
import { Segmented } from "../../../kit/Segmented";
import { CoverageBanner } from "../../../kit/States";
import { Table, type Column } from "../../../kit/Table";
import { featureSince } from "../../../lib/coverage";
import { formatNumber } from "../../../lib/format";
import { refIdentity } from "../../../lib/refs";
import { parseTimestamp, useTimeFormat } from "../../../lib/time";
import { useUrlEnum } from "../../../lib/url-state";
import { DEFAULT_LOOKUP_WINDOW, LOOKUP_WINDOWS } from "../vocabulary";

type TopPlayer = TopLookups["players"][number];
type StreamRow = LookupRow & { key: string };

const STREAM_HEIGHT = "520px";
const LOOKUP_FEATURES = ["subjects", "rollups"] as const;
const VIA_LABEL: Record<LookupVia | "both", string> = { key: "via key", bot: "via bot", both: "via key and bot" };

export function LookupsTab({ core }: { core: AccountCore }) {
  const discordId = core.account.discord_id;
  const [window, setWindow] = useUrlEnum("lookups", LOOKUP_WINDOWS, DEFAULT_LOOKUP_WINDOW);
  const summary = useQuery(lookupSummaryQuery(discordId, window));
  const top = useQuery(topLookupsQuery(discordId, window));
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-12 text-muted">Players this account looked up, through its keys and through bot commands it ran.</span>
        <Segmented label="Lookup window" value={window} options={LOOKUP_WINDOWS.map((value) => ({ value, label: value }))} onChange={setWindow} />
      </div>
      <QueryContent query={summary}>{(data) => <SummaryKpis summary={data} window={window} />}</QueryContent>
      <Section title="Top players" count={top.data === undefined ? undefined : <SampleLabel top={top.data} />} flush fetching={top.isFetching && top.data !== undefined}>
        <QueryContent query={top} isEmpty={(data) => data.players.length === 0} empty="No player lookups in this window.">
          {(data) => <TopPlayersTable players={data.players} />}
        </QueryContent>
      </Section>
      <LookupStream discordId={discordId} />
    </>
  );
}

function SummaryKpis({ summary, window }: { summary: LookupSummary; window: LookupWindow }) {
  const { coverage } = useBoot();
  const { time } = useTimeFormat();
  const since = featureSince(coverage, "body_subjects");
  const crossesBodyCapture = since === null || parseTimestamp(summary.from).getTime() < parseTimestamp(since).getTime();
  return (
    <>
      <CoverageNotices coverage={coverage} features={LOOKUP_FEATURES} from={summary.from} to={summary.to} />
      {crossesBodyCapture && (
        <CoverageBanner>
          Batch-body lookups {since === null ? "were not recorded yet" : `before ${time(since, { seconds: false, date: "always", zone: true })} were never captured`} (
          {formatNumber(summary.uncaptured_batches)} batch requests from this account in {window}).
        </CoverageBanner>
      )}
      <KpiStrip>
        <KpiTile label={<Badge variant="key">via key</Badge>} value={formatNumber(summary.via_key.player_mentions)} unit="mentions" sub={`${formatNumber(summary.via_key.requests)} requests · exact`} />
        <KpiTile label={<Badge variant="bot">via bot</Badge>} value={formatNumber(summary.via_bot.player_mentions)} unit="mentions" sub={`${formatNumber(summary.via_bot.requests)} commands this user ran · exact`} />
      </KpiStrip>
    </>
  );
}

function TopPlayersTable({ players }: { players: readonly TopPlayer[] }) {
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<TopPlayer>[]>(
    () => [
      { id: "player", header: "Player", cell: (row) => <EntityLink entity={row.player} /> },
      { id: "requests", header: "Lookups", cell: (row) => formatNumber(row.requests), numeric: true },
      { id: "via", header: "Via", cell: (row) => <Badge variant={row.via === "bot" ? "bot" : "key"}>{VIA_LABEL[row.via]}</Badge> },
      { id: "last", header: "Last", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.last, { seconds: false })}</span>, priority: 2 },
    ],
    [time],
  );
  return <Table label="Most looked-up players" columns={columns} rows={players} rowKey={(row) => refIdentity(row.player)} height="420px" />;
}

function LookupStream({ discordId }: { discordId: string }) {
  const stream = useInfiniteQuery(lookupStreamQuery(discordId));
  const rows = useMemo<StreamRow[]>(
    () => stream.data?.pages.flatMap((page, pageIndex) => page.items.map((item, index) => ({ ...item, key: `${pageIndex}:${index}` }))) ?? [],
    [stream.data],
  );
  const { time } = useTimeFormat();
  const columns = useMemo<readonly Column<StreamRow>[]>(
    () => [
      { id: "time", header: "Time", cell: (row) => <EntityLink entity={row.request} label={time(row.ts, { millis: true })} mono plain />, className: "whitespace-nowrap" },
      { id: "player", header: "Player", cell: (row) => <EntityLink entity={row.player === null ? { kind: "name", name: row.name ?? "" } : { kind: "player", uuid: row.player }} /> },
      { id: "form", header: "Form", cell: (row) => <Badge>{row.form}</Badge>, priority: 2 },
      { id: "via", header: "Via", cell: (row) => <Badge variant={row.via === "bot" ? "bot" : "key"}>{VIA_LABEL[row.via]}</Badge> },
    ],
    [time],
  );
  return (
    <Section title="Lookup stream" flush fetching={stream.isFetching && !stream.isFetchingNextPage && stream.data !== undefined}>
      <Table
        label="Lookups by this account, newest first"
        columns={columns}
        rows={rows}
        rowKey={(row) => row.key}
        virtual
        height={STREAM_HEIGHT}
        infinite={{ hasMore: stream.hasNextPage, loading: stream.isFetchingNextPage, onLoadMore: () => void stream.fetchNextPage() }}
        loading={stream.isPending}
        error={stream.error}
        onRetry={() => void stream.refetch()}
        empty="No player lookups by this account in the last 14 days."
      />
    </Section>
  );
}

function SampleLabel({ top }: { top: TopLookups }) {
  const { time } = useTimeFormat();
  if (top.exact || top.sample_from === null) return <>exact · {formatNumber(top.sample_size)} lookups</>;
  return (
    <>
      among the newest {formatNumber(top.sample_size)} lookups since {time(top.sample_from, { seconds: false, date: "always", zone: true })}
    </>
  );
}
