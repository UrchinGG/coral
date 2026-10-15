import type { InfiniteData, UseInfiniteQueryResult } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";

import { SCAN_AUTO_CONTINUE_HOURS } from "../../api/queryClient";
import type { Page, Subject } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";
import { listContextState } from "../../app/page-hotkeys";
import { Badge, MethodBadge, StatusPill } from "../../kit/Badges";
import { Button } from "../../kit/Button";
import { EntityLink } from "../../kit/EntityLink";
import { CoverageBanner, ErrorState } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import { refHref } from "../../lib/refs";
import { parseTimestamp, useTimeFormat } from "../../lib/time";
import { requestRef } from "./request-ref";
import { scanProgress, scanStateOf, type ScanProgress } from "./scan";

export type StreamQuery = UseInfiniteQueryResult<InfiniteData<Page<RequestRow>>, Error>;

type RequestStreamProps = {
  label: string;
  query: StreamQuery;
  requester?: boolean;
  empty: string;
};

const STREAM_HEIGHT = "520px";
const SUBJECTS_SHOWN = 3;

export function RequestStream({ label, query, requester = true, empty }: RequestStreamProps) {
  const navigate = useNavigate();
  const { time } = useTimeFormat();
  const pages = query.data?.pages;
  const rows = useMemo(() => pages?.flatMap((page) => page.items) ?? [], [pages]);
  const scan = scanStateOf(pages ?? []);
  const continuing = useAutoContinue(query, rows.length, scan.scannedTo);
  const columns = useStreamColumns(requester);
  const paths = rows.map((row) => refHref(requestRef(row)) ?? "");

  return (
    <div className="flex min-w-0 flex-col">
      {scan.clampedFrom !== null && (
        <div className="p-2">
          <CoverageBanner>Filters that need attribution start at {time(scan.clampedFrom, { seconds: false, date: "always", zone: true })}, when it was first recorded.</CoverageBanner>
        </div>
      )}
      {query.error !== null && rows.length > 0 && (
        <div className="p-2">
          <ErrorState error={query.error} onRetry={() => void query.fetchNextPage()} retrying={query.isFetchingNextPage} title="Loading more failed" />
        </div>
      )}
      <Table
        label={label}
        columns={columns}
        rows={rows}
        rowKey={(row) => row.request_id}
        onRowClick={(row) => navigate(paths[rows.indexOf(row)], { state: listContextState(paths, rows.indexOf(row)) })}
        virtual
        height={STREAM_HEIGHT}
        infinite={{ hasMore: query.hasNextPage, loading: query.isFetchingNextPage, onLoadMore: () => void query.fetchNextPage() }}
        loading={query.isPending || (rows.length === 0 && continuing.active)}
        error={rows.length === 0 ? query.error : null}
        onRetry={() => void query.refetch()}
        empty={empty}
      />
      <ScanFooter scannedTo={scan.scannedTo} continuing={continuing} hasMore={query.hasNextPage} onContinue={continuing.extend} />
    </div>
  );
}

function ScanFooter({ scannedTo, continuing, hasMore, onContinue }: { scannedTo: string | null; continuing: AutoContinue; hasMore: boolean; onContinue: () => void }) {
  const { time } = useTimeFormat();
  if (scannedTo === null || !hasMore) return null;
  const stamp = time(scannedTo, { seconds: false, date: "always" });
  return (
    <div className="flex flex-wrap items-center gap-2.5 border-t border-border px-3 py-2 text-12 text-subtle">
      <span>
        Scanned back to {stamp}
        {continuing.active ? " · continuing" : ""}
      </span>
      {continuing.exhausted && (
        <Button size="sm" onClick={onContinue}>
          Continue scanning (next {SCAN_AUTO_CONTINUE_HOURS} h)
        </Button>
      )}
    </div>
  );
}

type AutoContinue = ScanProgress & { extend: () => void };

function useAutoContinue(query: StreamQuery, loadedRows: number, scannedTo: string | null): AutoContinue {
  const [budgetStart, setBudgetStart] = useState(() => Date.now());
  const progress = scanProgress({ scannedTo, hasNextPage: query.hasNextPage, loadedRows, budgetStart });
  const { fetchNextPage, isFetching } = query;

  useEffect(() => {
    if (progress.active && !isFetching) void fetchNextPage();
  }, [progress.active, isFetching, fetchNextPage]);

  const extend = () => {
    if (scannedTo !== null) setBudgetStart(parseTimestamp(scannedTo).getTime());
  };
  return { ...progress, extend };
}

function useStreamColumns(requester: boolean): readonly Column<RequestRow>[] {
  const { time } = useTimeFormat();
  return useMemo(() => {
    const columns: Column<RequestRow>[] = [
      {
        id: "time",
        header: "Time",
        cell: (row) => <EntityLink entity={requestRef(row)} label={time(row.ts, { millis: true })} mono plain />,
        className: "whitespace-nowrap",
      },
      {
        id: "route",
        header: "Route",
        cell: (row) => (
          <span className="inline-flex items-center gap-1.5">
            <MethodBadge method={row.method} />
            <span className="font-mono text-[11.5px] whitespace-nowrap">{row.route ?? row.path}</span>
          </span>
        ),
      },
      { id: "subjects", header: "Subjects", cell: (row) => <SubjectChips subjects={row.subjects} bodySubjects={row.body_subjects} />, priority: 2 },
      { id: "status", header: "Status → outcome · cause", cell: (row) => <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} /> },
      { id: "latency", header: "Latency", cell: (row) => `${formatNumber(row.latency_ms)} ms`, numeric: true, priority: 2 },
      { id: "ip", header: "IP", cell: (row) => (row.ip === null ? <span className="text-subtle">none</span> : <EntityLink entity={row.ip} />), priority: 3 },
    ];
    if (!requester) return columns;
    return [...columns.slice(0, 3), { id: "requester", header: "Requester", cell: (row) => <Requester row={row} />, priority: 2 }, ...columns.slice(3)];
  }, [requester, time]);
}

function Requester({ row }: { row: RequestRow }) {
  if (row.actor === null) return <EntityLink entity={row.requester} />;
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      <EntityLink entity={row.requester} />
      <span className="text-subtle">for</span>
      <EntityLink entity={row.actor} />
    </span>
  );
}

function SubjectChips({ subjects, bodySubjects }: { subjects: readonly Subject[]; bodySubjects: number }) {
  const [expanded, setExpanded] = useState(false);
  if (subjects.length === 0 && bodySubjects === 0) return <span className="text-subtle">none</span>;
  const shown = expanded ? subjects : subjects.slice(0, SUBJECTS_SHOWN);
  const hidden = subjects.length - shown.length;
  return (
    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
      {shown.map((subject, index) => (
        <EntityLink key={`${subject.raw}:${index}`} entity={subject.player === null ? { kind: "name", name: subject.name ?? subject.raw } : { kind: "player", uuid: subject.player }} />
      ))}
      {hidden > 0 && (
        <button type="button" className="text-11 text-muted hover:text-fg" onClick={() => setExpanded(true)}>
          +{formatNumber(hidden)} more
        </button>
      )}
      {bodySubjects > 0 && <Badge title="Players named in the request body; open the request to see them">+{formatNumber(bodySubjects)} in body</Badge>}
    </span>
  );
}
