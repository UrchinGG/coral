import { useInfiniteQuery } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";

import { playerRequestsQuery, type LookupSelection } from "../../api/players";
import type { PlayerRequestRow } from "../../api/types/players";
import { useListHotkeys } from "../../app/page-hotkeys";
import { Badge, MethodBadge, StatusPill } from "../../kit/Badges";
import { Drawer } from "../../kit/Drawer";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { Section } from "../../kit/Section";
import { Table, type Column } from "../../kit/Table";
import { formatNumber, plural } from "../../lib/format";
import { SERVICE_LABEL } from "../../lib/refs";
import { useTimeFormat } from "../../lib/time";
import { useUrlParam } from "../../lib/url-state";
import { matchedValue, serviceOf, viaLabel } from "./stream-rows";

type RequestStreamProps = { uuid: string; selection: LookupSelection; requester: string | null; names: readonly string[] };

const STREAM_HEIGHT = "560px";

export function RequestStream({ uuid, selection, requester, names }: RequestStreamProps) {
  const stream = useInfiniteQuery(playerRequestsQuery(uuid, selection, requester));
  const rows = useMemo(() => stream.data?.pages.flatMap((page) => page.items) ?? [], [stream.data]);
  const [selectedId, setSelectedId] = useUrlParam("row");
  const columns = useStreamColumns(uuid, names);
  const rowKey = useCallback((row: PlayerRequestRow) => row.request_id, []);
  const selected = rows.find((row) => row.request_id === selectedId) ?? null;
  const loadMore = useCallback(() => void stream.fetchNextPage(), [stream]);
  useListHotkeys({ rows, rowKey, selectedKey: selectedId, onSelect: setSelectedId, onOpen: (row) => setSelectedId(row.request_id) });

  return (
    <Section title="Stream" count={rows.length === 0 ? undefined : `${formatNumber(rows.length)} loaded`} flush fetching={stream.isFetching && rows.length > 0}>
      <Table
        label="Requests that name this player"
        columns={columns}
        rows={rows}
        rowKey={rowKey}
        selectedKey={selectedId}
        onRowClick={(row) => setSelectedId(row.request_id)}
        virtual
        height={STREAM_HEIGHT}
        loading={stream.isPending}
        error={stream.error}
        onRetry={() => void stream.refetch()}
        infinite={{ hasMore: stream.hasNextPage && !stream.isPlaceholderData, loading: stream.isFetchingNextPage, onLoadMore: loadMore }}
        empty="No request names this player in this window with the selected forms."
      />
      <Drawer open={selected !== null} onClose={() => setSelectedId(null)} title="Request preview">
        {selected !== null && <RequestPreview row={selected} uuid={uuid} names={names} />}
      </Drawer>
    </Section>
  );
}

function useStreamColumns(uuid: string, names: readonly string[]): readonly Column<PlayerRequestRow>[] {
  const { time } = useTimeFormat();
  return useMemo(
    () => [
      {
        id: "time",
        header: "Time",
        cell: (row) => (
          <EntityLink entity={{ kind: "request", request_id: row.request_id, ts: row.ts }} label={time(row.ts, { millis: true })} mono plain />
        ),
        className: "whitespace-nowrap",
      },
      {
        id: "route",
        header: "Route",
        cell: (row) => (
          <span className="inline-flex items-center gap-1.5">
            <MethodBadge method={row.method} />
            <span className="font-mono text-[11.5px]">{row.route ?? row.path}</span>
          </span>
        ),
      },
      { id: "match", header: "Form + match", cell: (row) => <FormMatch row={row} uuid={uuid} names={names} /> },
      { id: "requester", header: "Requester", cell: (row) => <Requester row={row} />, priority: 2 },
      { id: "status", header: "Status", cell: (row) => <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} /> },
      { id: "latency", header: "Latency", cell: (row) => `${formatNumber(row.latency_ms)} ms`, numeric: true, priority: 2 },
    ],
    [time, uuid, names],
  );
}

function FormMatch({ row, uuid, names }: { row: PlayerRequestRow; uuid: string; names: readonly string[] }) {
  const value = matchedValue(row, uuid, names);
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <Badge>{row.matched_form}</Badge>
      {value !== null && <span className="font-mono text-[11.5px] break-all">"{value}"</span>}
      <MatchBadge row={row} />
    </span>
  );
}

function MatchBadge({ row }: { row: PlayerRequestRow }) {
  switch (row.match) {
    case "name_only":
      return (
        <Badge variant="byname" title="Matched by name inside the window this player held it; never resolved to a UUID">
          by name
        </Badge>
      );
    case "display_name":
      return <span className="text-11 text-subtle">display name</span>;
    case "resolved_name":
      return <span className="text-11 text-subtle">resolved</span>;
    case "uuid":
    case undefined:
      return <span className="text-11 text-subtle">uuid</span>;
  }
}

function Requester({ row }: { row: PlayerRequestRow }) {
  const service = serviceOf(row);
  if (service !== null && row.actor !== null) {
    return (
      <span className="inline-flex flex-wrap items-center gap-1.5">
        <EntityLink entity={row.actor} />
        <Badge variant="bot" title={`Looked up through ${SERVICE_LABEL[service]} on this user's behalf`}>
          {viaLabel(service)}
        </Badge>
      </span>
    );
  }
  return (
    <span className="inline-flex flex-wrap items-center gap-1.5">
      <EntityLink entity={row.requester} />
      {row.actor !== null && <EntityLink entity={row.actor} />}
    </span>
  );
}

function RequestPreview({ row, uuid, names }: { row: PlayerRequestRow; uuid: string; names: readonly string[] }) {
  const { time } = useTimeFormat();
  return (
    <KeyValue
      items={[
        { label: "Request", value: <EntityLink entity={{ kind: "request", request_id: row.request_id, ts: row.ts }} mono /> },
        { label: "Time", value: time(row.ts, { millis: true, zone: true, date: "always" }) },
        { label: "Status", value: <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} /> },
        { label: "Path", value: <span className="font-mono break-all">{row.path}</span> },
        { label: "Query", value: <span className="font-mono break-all">{row.query ?? "none"}</span> },
        { label: "Matched", value: <FormMatch row={row} uuid={uuid} names={names} /> },
        { label: "Requester", value: <Requester row={row} /> },
        { label: "Key kind", value: <span className="font-mono">{row.key_kind}</span> },
        { label: "IP", value: row.ip === null ? "none" : <EntityLink entity={row.ip} /> },
        { label: "User agent", value: <span className="font-mono break-all">{row.user_agent ?? "none"}</span> },
        { label: "Batch body", value: row.body_subjects === 0 ? "none" : plural(row.body_subjects, "player") },
        { label: "Latency", value: `${formatNumber(row.latency_ms)} ms` },
      ]}
    />
  );
}
