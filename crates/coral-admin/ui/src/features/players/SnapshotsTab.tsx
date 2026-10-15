import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";

import { playerSnapshotsQuery, snapshotAtQuery } from "../../api/players";
import type { SnapshotDay, SnapshotRow } from "../../api/types/players";
import { Badge } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { EntityLink } from "../../kit/EntityLink";
import { LazyJsonView } from "../../kit/LazyJsonView";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import { SkeletonLines } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { cx } from "../../lib/cx";
import { formatNumber, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { useUrlParam } from "../../lib/url-state";

const SNAPSHOT_LIST_HEIGHT = "520px";
const JSON_INITIAL_DEPTH = 1;
const HEAT_LEVELS = [0.25, 0.45, 0.7, 1] as const;

export function SnapshotsTab({ uuid }: { uuid: string }) {
  const [day, setDay] = useUrlParam("day");
  const [at, setAt] = useUrlParam("at");
  const snapshots = useInfiniteQuery(playerSnapshotsQuery(uuid, day));
  const rows = useMemo(() => snapshots.data?.pages.flatMap((page) => page.rows) ?? [], [snapshots.data]);
  const days = snapshots.data?.pages[0]?.days ?? [];
  const shown = at ?? rows[0]?.ts ?? null;
  const loadMore = useCallback(() => void snapshots.fetchNextPage(), [snapshots]);
  const columns = useSnapshotColumns();

  return (
    <div className="flex flex-col gap-4">
      {snapshots.data !== undefined || snapshots.error === null ? (
        <DayStrip days={days} selected={day} onSelect={(next) => setDay(next)} loading={snapshots.isPending} />
      ) : null}
      <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(0,380px)_minmax(0,1fr)]">
        <Section title={day === null ? "Newest snapshots" : `Snapshots on ${day}`} count={rows.length === 0 ? undefined : formatNumber(rows.length)} flush fetching={snapshots.isFetching && rows.length > 0}>
          <Table
            label="Snapshot timestamps"
            columns={columns}
            rows={rows}
            rowKey={(row) => row.ts}
            selectedKey={shown}
            onRowClick={(row) => setAt(row.ts)}
            virtual
            height={SNAPSHOT_LIST_HEIGHT}
            loading={snapshots.isPending}
            error={snapshots.error}
            onRetry={() => void snapshots.refetch()}
            infinite={{ hasMore: snapshots.hasNextPage && !snapshots.isPlaceholderData, loading: snapshots.isFetchingNextPage, onLoadMore: loadMore }}
            empty="This player has no snapshots."
          />
        </Section>
        {shown === null ? null : <SnapshotView uuid={uuid} ts={shown} />}
      </div>
    </div>
  );
}

function SnapshotView({ uuid, ts }: { uuid: string; ts: string }) {
  const { time } = useTimeFormat();
  const snapshot = useQuery(snapshotAtQuery(uuid, ts));
  return (
    <Section title="Snapshot" count={time(ts, { millis: true, zone: true, date: "always" })} fetching={snapshot.isFetching && snapshot.data !== undefined}>
      <QueryContent query={snapshot} skeleton={<SkeletonLines />}>
        {(data) => <LazyJsonView value={data.data} label="Snapshot" initialDepth={JSON_INITIAL_DEPTH} links={{ players: true }} />}
      </QueryContent>
    </Section>
  );
}

function DayStrip({ days, selected, onSelect, loading }: { days: readonly SnapshotDay[]; selected: string | null; onSelect: (day: string | null) => void; loading: boolean }) {
  const busiest = days.reduce((most, day) => Math.max(most, day.count), 0);
  const ordered = useMemo(() => [...days].reverse(), [days]);
  return (
    <Section
      title="Days with snapshots"
      count={loading ? undefined : plural(days.length, "day")}
      actions={
        selected === null ? undefined : (
          <Button size="sm" variant="ghost" onClick={() => onSelect(null)}>
            Show newest
          </Button>
        )
      }
    >
      {loading ? (
        <SkeletonLines widths={["90%"]} />
      ) : (
        <div className="flex max-w-full gap-[3px] overflow-x-auto pb-1" role="group" aria-label="Snapshot days, oldest first">
          {ordered.map((day) => (
            <button
              key={day.day}
              type="button"
              title={`${day.day}: ${plural(day.count, "snapshot")}`}
              aria-label={`${day.day}, ${plural(day.count, "snapshot")}`}
              aria-pressed={day.day === selected}
              onClick={() => onSelect(day.day)}
              style={{ opacity: heat(day.count, busiest) }}
              className={cx("h-6 w-2 shrink-0 rounded-[2px] bg-entity-player", day.day === selected && "outline-2 outline-offset-1 outline-accent")}
            />
          ))}
        </div>
      )}
    </Section>
  );
}

function useSnapshotColumns(): readonly Column<SnapshotRow>[] {
  const { time } = useTimeFormat();
  return useMemo(
    () => [
      { id: "taken", header: "Taken", cell: (row) => <span className="font-mono text-[11.5px] whitespace-nowrap">{time(row.ts, { seconds: true, date: "always" })}</span> },
      {
        id: "username",
        header: "Username",
        cell: (row) =>
          row.username === null ? <span className="text-subtle">none</span> : <EntityLink entity={{ kind: "name", name: row.username.toLowerCase() }} label={row.username} mono />,
      },
      { id: "source", header: "Source", cell: (row) => row.source ?? <span className="text-subtle">unknown</span>, priority: 2 },
      { id: "baseline", header: "Kind", cell: (row) => (row.baseline ? <Badge>baseline</Badge> : <span className="text-subtle">delta</span>), priority: 2 },
    ],
    [time],
  );
}

function heat(count: number, busiest: number): number {
  if (busiest === 0) return HEAT_LEVELS[0];
  const level = Math.min(HEAT_LEVELS.length - 1, Math.floor((count / busiest) * HEAT_LEVELS.length));
  return HEAT_LEVELS[level];
}
