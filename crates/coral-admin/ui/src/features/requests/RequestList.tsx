import { useInfiniteQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import { requestListQuery } from "../../api/requests";
import { SCAN_AUTO_CONTINUE_HOURS } from "../../api/queryClient";
import type { RequestQuery } from "../../api/types/request-explorer";
import type { RequestRow } from "../../api/types/requests";
import type { Coverage } from "../../api/types/session";
import { listContextState, useListHotkeys } from "../../app/page-hotkeys";
import { MethodBadge, StatusPill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { Section } from "../../kit/Section";
import { ErrorState } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import type { CoverageFeature } from "../../lib/coverage";
import { requestPath } from "./paths";
import { RequestDrawer } from "./RequestDrawer";
import { IpCell, LatencyText, RequesterCell, RouteCell, SubjectsCell } from "./request-cells";
import { scannedHours, scanProgress, shouldAutoContinue, type ScanProgress } from "./scan";
import { useRequestTail, type TailSeed } from "./useRequestTail";

type RequestListProps = {
  query: RequestQuery;
  live: boolean;
  topMs: number;
  coverage: Coverage;
  features: readonly CoverageFeature[];
  selected: string | null;
  onSelect: (requestId: string | null) => void;
  onRebase: () => void;
};

const LIST_HEIGHT = "min(72vh, 760px)";
const LIST_CONTEXT_RADIUS = 100;
const NIL_REQUEST_ID = "00000000000000000000000000000000";
const MAX_TAIL_ROWS = 5_000;

export function RequestList({ query, live, topMs, coverage, features, selected, onSelect, onRebase }: RequestListProps) {
  const navigate = useNavigate();
  const list = useInfiniteQuery(requestListQuery(query));
  const settled = list.data !== undefined && !list.isPlaceholderData;
  const pages = useMemo(() => list.data?.pages ?? [], [list.data]);
  const seed = useMemo<TailSeed | null>(() => (settled ? (pages[0]?.items[0] ?? { ts: new Date(topMs).toISOString(), request_id: NIL_REQUEST_ID }) : null), [settled, pages, topMs]);
  const tail = useRequestTail(query, live, seed);
  const rows = useMemo(() => mergeRows(tail.rows, pages.flatMap((page) => page.items)), [tail.rows, pages]);
  const progress = scanProgress(pages, topMs);
  const [budget, setBudget] = useState({ signature: "", hours: SCAN_AUTO_CONTINUE_HOURS });
  const signature = JSON.stringify(query);
  const budgetHours = budget.signature === signature ? budget.hours : SCAN_AUTO_CONTINUE_HOURS;
  const withinBudget = !progress.partial || shouldAutoContinue(progress, budgetHours);
  const columns = useListColumns();
  const selectedRow = rows.find((row) => row.request_id === selected) ?? null;

  const { hasNextPage, isFetchingNextPage, fetchNextPage, error } = list;
  const continueScan = shouldAutoContinue(progress, budgetHours);
  const fetchMore = () => {
    if (settled && hasNextPage && !isFetchingNextPage && withinBudget) void fetchNextPage();
  };
  const open = (row: RequestRow) => {
    const index = rows.indexOf(row);
    const nearby = rows.slice(Math.max(0, index - LIST_CONTEXT_RADIUS), index + LIST_CONTEXT_RADIUS + 1);
    navigate(requestPath(row), { state: listContextState(nearby.map(requestPath), nearby.indexOf(row)) });
  };

  useEffect(() => {
    if (settled && continueScan && hasNextPage && !isFetchingNextPage && error === null) void fetchNextPage();
  }, [settled, continueScan, hasNextPage, isFetchingNextPage, error, fetchNextPage]);
  useEffect(() => {
    if (tail.rows.length >= MAX_TAIL_ROWS) onRebase();
  }, [tail.rows.length, onRebase]);
  useListHotkeys({ rows, rowKey, selectedKey: selectedRow?.request_id ?? null, onSelect, onOpen: open });

  const clampedFrom = pages[0]?.clamped_from ?? null;
  const scanStatus = (
    <ScanStatus
      progress={progress}
      withinBudget={withinBudget}
      exhausted={!hasNextPage}
      onContinue={() => setBudget({ signature, hours: scannedHours(progress) + SCAN_AUTO_CONTINUE_HOURS })}
    />
  );
  return (
    <>
      <CoverageNotices coverage={coverage} features={features} from={query.from ?? new Date(topMs).toISOString()} to={query.to ?? new Date(topMs).toISOString()} clampedFrom={clampedFrom} />
      {tail.error !== null && <ErrorState error={tail.error} title="Live tail failed; retrying every 5 s" />}
      <Section title="Requests" count={list.data === undefined ? undefined : `${formatNumber(rows.length)} loaded`} flush fetching={list.isFetching && list.data !== undefined && !list.isFetchingNextPage}>
        <Table
          label="Requests"
          columns={columns}
          rows={rows}
          rowKey={rowKey}
          selectedKey={selectedRow?.request_id ?? null}
          onRowClick={(row) => onSelect(row.request_id)}
          height={LIST_HEIGHT}
          virtual
          loading={list.isPending}
          error={error}
          onRetry={() => void (list.data === undefined ? list.refetch() : fetchNextPage())}
          empty={progress.partial ? scanStatus : "No requests match these filters in this window."}
          infinite={{ hasMore: hasNextPage, loading: isFetchingNextPage, onLoadMore: fetchMore }}
          footer={scanStatus}
        />
      </Section>
      {selectedRow !== null && <RequestDrawer row={selectedRow} onClose={() => onSelect(null)} onOpen={() => open(selectedRow)} />}
    </>
  );
}

function ScanStatus({ progress, withinBudget, exhausted, onContinue }: { progress: ScanProgress; withinBudget: boolean; exhausted: boolean; onContinue: () => void }) {
  const { time } = useTimeFormat();
  if (exhausted || !progress.partial || progress.scannedTo === null) return <KeyboardHint />;
  if (withinBudget) return <span>Scanned back to {time(progress.scannedTo, { seconds: false })} · continuing</span>;
  return (
    <span className="inline-flex flex-wrap items-center gap-2">
      Scanned {formatNumber(Math.floor(scannedHours(progress)))} h back to {time(progress.scannedTo, { seconds: false, date: "always" })} without reaching the end of the window.
      <Button size="sm" onClick={onContinue}>
        Continue scanning (next {SCAN_AUTO_CONTINUE_HOURS} h)
      </Button>
    </span>
  );
}

function KeyboardHint() {
  return <span>j / k move · Enter preview · o open · Esc close</span>;
}

function useListColumns(): Column<RequestRow>[] {
  const { time } = useTimeFormat();
  return [
    { id: "time", header: "Time", cell: (row) => <span className="font-mono text-12 whitespace-nowrap">{time(row.ts, { millis: true })}</span> },
    { id: "method", header: "Method", cell: (row) => <MethodBadge method={row.method} /> },
    { id: "route", header: "Route", cell: (row) => <RouteCell row={row} /> },
    { id: "subjects", header: "Subjects", cell: (row) => <SubjectsCell row={row} />, priority: 2 },
    { id: "requester", header: "Requester", cell: (row) => <RequesterCell row={row} />, priority: 2 },
    { id: "status", header: "Status → outcome · cause", cell: (row) => <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} /> },
    { id: "latency", header: "Latency", cell: (row) => <LatencyText ms={row.latency_ms} />, numeric: true, priority: 2 },
    { id: "ip", header: "IP", cell: (row) => <IpCell row={row} />, priority: 3 },
    { id: "country", header: "Country", cell: (row) => <span className="font-mono text-12">{row.country ?? "—"}</span>, priority: 3 },
  ];
}

function mergeRows(tail: readonly RequestRow[], listed: readonly RequestRow[]): RequestRow[] {
  const seen = new Set<string>();
  return [...tail, ...listed].filter((row) => {
    if (seen.has(row.request_id)) return false;
    seen.add(row.request_id);
    return true;
  });
}

function rowKey(row: RequestRow): string {
  return row.request_id;
}
