import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { ExternalLink } from "lucide-react";
import { Link } from "react-router-dom";

import { requestFullQuery } from "../../api/requests";
import type { Subject } from "../../api/types/refs";
import type { RequestFull } from "../../api/types/request-full";
import type { RequestRow } from "../../api/types/requests";
import { MethodBadge, StatusPill } from "../../kit/Badges";
import { ClampText } from "../../kit/ClampText";
import { CopyField } from "../../kit/Copy";
import { Drawer } from "../../kit/Drawer";
import { EntityLink } from "../../kit/EntityLink";
import { KeyValue } from "../../kit/KeyValue";
import { ErrorState, SkeletonLines } from "../../kit/States";
import { formatBytes, plural } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { DetailStateNotice } from "./DetailStateNotice";
import { requestPath } from "./paths";
import { KeyCell, LatencyText, RequesterCell, RouteCell, SubjectChip } from "./request-cells";

type RequestDrawerProps = {
  row: RequestRow;
  onClose: () => void;
  onOpen: () => void;
};

export function RequestDrawer({ row, onClose, onOpen }: RequestDrawerProps) {
  const { time } = useTimeFormat();
  const full = useQuery(requestFullQuery(row.request_id, row.ts));
  return (
    <Drawer
      open
      onClose={onClose}
      title={
        <span className="flex min-w-0 items-center gap-2">
          <MethodBadge method={row.method} />
          <RouteCell row={row} />
        </span>
      }
      actions={
        <Link
          to={requestPath(row)}
          onClick={(event) => {
            event.preventDefault();
            onOpen();
          }}
          className="inline-flex h-6 items-center gap-1.5 rounded-md border border-accent bg-accent px-2 text-11 font-semibold whitespace-nowrap text-accent-ink hover:bg-accent-hover"
        >
          <ExternalLink className="size-3" />
          Open full request
        </Link>
      }
    >
      <div className="font-mono text-12 break-all">{row.query === null ? row.path : `${row.path}?${row.query}`}</div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-12">
        <StatusPill status={row.status} outcomeStatus={row.outcome_status} cause={row.cause} />
        <LatencyText ms={row.latency_ms} />
        <span className="font-mono">{time(row.ts, { millis: true, date: "always", zone: true })}</span>
      </div>
      <KeyValue
        items={[
          { key: "id", label: "Request id", value: <CopyField value={row.request_id} label="Copy request id" /> },
          { key: "requester", label: "Requester", value: <RequesterCell row={row} /> },
          { key: "key", label: "Key", value: <KeyCell row={row} /> },
          { key: "ip", label: "IP", value: row.ip === null ? <span className="text-subtle">not recorded</span> : <EntityLink entity={row.ip} /> },
          { key: "country", label: "Country", value: <span className="font-mono">{row.country ?? "—"}</span> },
          { key: "agent", label: "User agent", value: <span className="font-mono">{row.user_agent ?? "—"}</span> },
          ...(row.error === null ? [] : [{ key: "error", label: "Response", value: <ClampText className="font-mono text-12">{row.error}</ClampText> }]),
        ]}
      />
      <DrawerPlayers row={row} involved={full.data?.subjects ?? null} />
      <DetailSummary query={full} />
    </Drawer>
  );
}

function DrawerPlayers({ row, involved }: { row: RequestRow; involved: readonly Subject[] | null }) {
  const subjects = involved ?? row.subjects;
  const unlisted = involved === null ? row.body_subjects : 0;
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Players</span>
      {subjects.length === 0 && unlisted === 0 ? (
        <span className="text-12 text-subtle">No player subjects</span>
      ) : (
        <div className="flex max-h-[180px] flex-wrap gap-x-2.5 gap-y-1 overflow-y-auto">
          {subjects.map((subject, index) => (
            <SubjectChip key={`${subject.form}:${subject.raw}:${index}`} subject={subject} />
          ))}
          {unlisted > 0 && (
            <span className="text-12 text-muted">
              {subjects.length > 0 && "and "}
              {plural(unlisted, "player")} in the body
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function DetailSummary({ query }: { query: UseQueryResult<RequestFull> }) {
  if (query.data === undefined) {
    if (query.error !== null) return <ErrorState error={query.error} onRetry={() => void query.refetch()} title="Stored detail unavailable" />;
    return <SkeletonLines widths={["60%"]} />;
  }
  const detail = query.data;
  if (detail.detail_state !== "present") return <DetailStateNotice state={detail.detail_state} />;
  const headers = Object.keys(detail.headers ?? {}).length;
  const body = detail.body_bytes === null || detail.body === null ? "no body" : `body ${formatBytes(detail.body_bytes)}${detail.body_complete ? ", complete" : ", partly stored"}`;
  return (
    <p className="m-0 text-12 text-muted">
      {plural(headers, "header")} · {body}
      {detail.server_error === null ? "" : " · server error recorded"}. The full view shows all of it.
    </p>
  );
}
