import { useQuery } from "@tanstack/react-query";
import { ChevronRight, Clock } from "lucide-react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import { requestFullQuery } from "../../api/requests";
import type { RequestFull } from "../../api/types/request-full";
import { useCopyHotkey, useListStepping, type ListStep } from "../../app/page-hotkeys";
import { MethodBadge, StatusPill } from "../../kit/Badges";
import { CopyButton, CopyField } from "../../kit/Copy";
import { QueryContent } from "../../kit/QueryContent";
import { SkeletonLines } from "../../kit/States";
import { formatMs } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { DetailStateNotice } from "./DetailStateNotice";
import { curlFor, requestJson } from "./request-copy";
import { RequestMain } from "./RequestMain";
import { RequestRail } from "./RequestRail";

export function RequestPage() {
  const { requestId = "" } = useParams();
  const [params] = useSearchParams();
  const full = useQuery(requestFullQuery(requestId, params.get("t")));
  useCopyHotkey(full.data?.url ?? null, "URL");

  return (
    <>
      <Breadcrumbs requestId={requestId} />
      <QueryContent query={full} skeleton={<SkeletonLines widths={["70%", "45%", "60%", "40%"]} />}>
        {(request) => <RequestDetail full={request} />}
      </QueryContent>
    </>
  );
}

function Breadcrumbs({ requestId }: { requestId: string }) {
  const stepping = useListStepping();
  return (
    <nav aria-label="Breadcrumb" className="flex flex-wrap items-center justify-between gap-2 text-12 text-muted">
      <span className="flex min-w-0 items-center gap-1">
        <Link to="/requests" className="text-accent hover:underline">
          Requests
        </Link>
        <ChevronRight className="size-3 shrink-0" />
        <span className="font-mono break-all">{requestId}</span>
      </span>
      {(stepping.previous !== null || stepping.next !== null) && (
        <span className="flex items-center gap-3">
          {stepping.previous !== null && <StepLink step={stepping.previous} label="[ previous in list" />}
          {stepping.next !== null && <StepLink step={stepping.next} label="next in list ]" />}
        </span>
      )}
    </nav>
  );
}

function StepLink({ step, label }: { step: ListStep; label: string }) {
  return (
    <Link to={step.path} state={step.state} className="text-accent hover:underline">
      {label}
    </Link>
  );
}

function RequestDetail({ full }: { full: RequestFull }) {
  return (
    <>
      <RequestHeader full={full} />
      {full.detail_state !== "present" && <DetailStateNotice state={full.detail_state} />}
      <div className="grid grid-cols-[minmax(0,1fr)_340px] items-start gap-5 max-xl:grid-cols-[minmax(0,1fr)_300px] max-lg:grid-cols-[minmax(0,1fr)]">
        <RequestMain full={full} />
        <RequestRail full={full} />
      </div>
    </>
  );
}

function RequestHeader({ full }: { full: RequestFull }) {
  const { time, relative } = useTimeFormat();
  const { request } = full;
  const curl = curlFor(full);
  const curlNote =
    full.detail_state === "present"
      ? curl.omittedHeaders.length === 0
        ? "Replays every stored header and the stored body"
        : `Leaves out headers a shell cannot carry: ${curl.omittedHeaders.join(", ")}`
      : "Headers and body were not stored; this replays only the method and URL";
  return (
    <header className="flex flex-col gap-2.5 rounded-md border border-border bg-surface px-4 py-3.5">
      <div className="flex items-start gap-2.5">
        <MethodBadge method={request.method} />
        <h1 className="m-0 min-w-0 font-mono text-15 font-medium break-all">{full.url}</h1>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <CopyButton text={full.url}>Copy URL</CopyButton>
        <span title={curlNote}>
          <CopyButton text={curl.command}>Copy curl</CopyButton>
        </span>
        <CopyButton text={() => requestJson(full)}>Copy JSON</CopyButton>
        {curl.omittedHeaders.length > 0 && <span className="text-11 text-warning">curl leaves out: {curl.omittedHeaders.join(", ")}</span>}
      </div>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-12 text-muted">
        <StatusPill status={request.status} outcomeStatus={request.outcome_status} cause={request.cause} />
        <span className="inline-flex items-center gap-1">
          <Clock className="size-3" />
          <span className="font-mono">{formatMs(request.latency_ms)}</span>
        </span>
        <span>
          <span className="font-mono text-fg">{time(request.ts, { millis: true, date: "always", zone: true })}</span> <span className="text-subtle">({relative(request.ts)})</span>
        </span>
        <span>
          route <span className="font-mono text-fg">{request.route ?? "unmatched"}</span>
        </span>
        <span>
          key kind <span className="font-mono text-fg">{request.key_kind}</span>
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-12">
        <span className="text-subtle">request id</span>
        <CopyField value={request.request_id} label="Copy request id" />
      </div>
    </header>
  );
}
