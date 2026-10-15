import { useInfiniteQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";

import { accountListQuery } from "../../api/accounts";
import type { AccountRow } from "../../api/types/accounts";
import { Pill } from "../../kit/Badges";
import { EntityLink } from "../../kit/EntityLink";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { Select } from "../../kit/Select";
import { Table, type Column } from "../../kit/Table";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { useUrlEnum } from "../../lib/url-state";
import { ANY, CHOICES, DEFAULT_DIRECTION, DEFAULT_SORT, DIRECTIONS, FLAGS, listFiltersFrom, RANKS, SORT_VALUES, SORTS, type Choice, type FlagFilter } from "./list-filters";
import { accessLabel, LICENSE_TONE } from "./vocabulary";

const FLAG_LABEL: Record<FlagFilter, string> = { locked: "Key locked", has_key: "Has API key", has_dev_key: "Has developer key", starfish: "Starfish license" };
const LIST_HEIGHT = "calc(100dvh - 260px)";

export function AccountsPage() {
  const filters = useListFilters();
  const list = useInfiniteQuery(accountListQuery(filters.value));
  const rows = useMemo(() => list.data?.pages.flatMap((page) => page.items) ?? [], [list.data]);
  const columns = useColumns();
  return (
    <>
      <div className="flex flex-col gap-1">
        <h1 className="text-22 font-semibold">Accounts</h1>
        <p className="m-0 max-w-[70ch] text-muted">Every Coral account, with its traffic over the last 24 hours. To open one account, search for its Discord id or name, a Minecraft name or a key in the search bar.</p>
      </div>
      <div className="flex flex-wrap items-end gap-3">{filters.controls}</div>
      <Section title="Accounts" count={rows.length === 0 ? undefined : `${formatNumber(rows.length)} loaded`} flush fetching={list.isFetching && !list.isFetchingNextPage && list.data !== undefined}>
        <Table
          label="Accounts"
          columns={columns}
          rows={rows}
          rowKey={(row) => row.account.discord_id}
          virtual
          height={LIST_HEIGHT}
          infinite={{ hasMore: list.hasNextPage, loading: list.isFetchingNextPage, onLoadMore: () => void list.fetchNextPage() }}
          loading={list.isPending}
          error={list.error}
          onRetry={() => void list.refetch()}
          empty="No account matches these filters."
        />
      </Section>
    </>
  );
}

function useListFilters() {
  const [search] = useSearchParams();
  const [sort, setSort] = useUrlEnum("sort", SORT_VALUES, DEFAULT_SORT);
  const [dir, setDir] = useUrlEnum("dir", DIRECTIONS, DEFAULT_DIRECTION);
  const [rank, setRank] = useUrlEnum("rank", RANKS, ANY);
  const flags = {
    locked: useUrlEnum("locked", CHOICES, ANY),
    has_key: useUrlEnum("has_key", CHOICES, ANY),
    has_dev_key: useUrlEnum("has_dev_key", CHOICES, ANY),
    starfish: useUrlEnum("starfish", CHOICES, ANY),
  } satisfies Record<FlagFilter, readonly [Choice, (value: Choice) => void]>;
  const controls = (
    <>
      <Select label="Sort by" value={sort} options={SORTS} onChange={setSort} />
      <Segmented label="Sort direction" value={dir} options={DIRECTIONS.map((direction) => ({ value: direction, label: direction === "desc" ? "Descending" : "Ascending" }))} onChange={setDir} />
      <Select label="Access" value={rank} options={RANKS.map((level) => ({ value: level, label: level === ANY ? "Any" : `${accessLabel(Number(level))} or higher` }))} onChange={setRank} />
      {FLAGS.map((name) => (
        <Select key={name} label={FLAG_LABEL[name]} value={flags[name][0]} options={CHOICES.map((choice) => ({ value: choice, label: choice === ANY ? "Any" : choice === "yes" ? "Yes" : "No" }))} onChange={flags[name][1]} />
      ))}
    </>
  );
  return { value: listFiltersFrom(search), controls };
}

function useColumns(): readonly Column<AccountRow>[] {
  const { time, relative } = useTimeFormat();
  return useMemo(
    () => [
      { id: "account", header: "Account", cell: (row) => <EntityLink entity={row.account} /> },
      { id: "member", header: "Member", cell: (row) => <span className="font-mono">#{row.account.member_id}</span>, numeric: true, priority: 2 },
      { id: "minecraft", header: "Minecraft", cell: (row) => <Minecraft row={row} />, priority: 2 },
      { id: "access", header: "Access", cell: (row) => (row.access_level > 0 ? <Pill tone="accent">{accessLabel(row.access_level)}</Pill> : <span className="text-subtle">{accessLabel(row.access_level)}</span>) },
      { id: "key", header: "Key", cell: (row) => <KeyState row={row} /> },
      { id: "standing", header: "Standing", cell: (row) => <Standing row={row} />, priority: 2 },
      { id: "requests", header: "Requests 24 h", cell: (row) => <Requests row={row} />, numeric: true },
      { id: "lifetime", header: "Lifetime", cell: (row) => formatNumber(row.request_count), numeric: true, priority: 3 },
      { id: "last", header: "Last active", cell: (row) => (row.last_active === null ? <span className="text-subtle">not in 24 h</span> : relative(row.last_active)), priority: 2 },
      { id: "joined", header: "Joined", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.joined, { seconds: false, date: "always" })}</span>, priority: 3 },
    ],
    [time, relative],
  );
}

function Minecraft({ row }: { row: AccountRow }) {
  if (row.minecraft === null) return <span className="text-subtle">none</span>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <EntityLink entity={row.minecraft} />
      {row.alts > 0 && <span className="text-11 text-subtle">+{plural(row.alts, "alt")}</span>}
    </span>
  );
}

function KeyState({ row }: { row: AccountRow }) {
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {!row.has_key ? <Pill>no key</Pill> : row.key_locked ? <Pill tone="danger">locked</Pill> : <Pill tone="ok">active</Pill>}
      {row.has_dev_key && <Pill tone="info">dev key</Pill>}
    </span>
  );
}

function Standing({ row }: { row: AccountRow }) {
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {row.active_strikes > 0 && <Pill tone="danger">{plural(row.active_strikes, "strike")}</Pill>}
      {row.tagging_disabled && <Pill tone="warning">no tagging</Pill>}
      {row.starfish_license !== null && <Pill tone={LICENSE_TONE[row.starfish_license] ?? "neutral"}>Starfish {row.starfish_license}</Pill>}
    </span>
  );
}

function Requests({ row }: { row: AccountRow }) {
  return (
    <span className="inline-flex items-baseline gap-1.5">
      {formatNumber(row.requests_24h)}
      {row.rate_limited_24h > 0 && <span className="text-11 text-limited">{formatNumber(row.rate_limited_24h)} limited</span>}
    </span>
  );
}
