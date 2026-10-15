import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useMemo } from "react";

import { playerRequestersQuery } from "../../api/players";
import type { PlayerCore, PlayerRequesters } from "../../api/types/players";
import type { Coverage } from "../../api/types/session";
import { useSession } from "../../app/boot";
import { Button } from "../../kit/Button";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { Section } from "../../kit/Section";
import { Segmented } from "../../kit/Segmented";
import { CoverageBanner, ErrorState } from "../../kit/States";
import { formatNumber } from "../../lib/format";
import { parseTimestamp, useTimeFormat } from "../../lib/time";
import { useUrlParam } from "../../lib/url-state";
import { FormChips } from "./FormChips";
import { PLAYER_WINDOWS, useLookupSelection } from "./player-state";
import { currentName, formerNames } from "./profile";
import { RequestersTable } from "./RequestersTable";
import { RequestStream } from "./RequestStream";

type RequestsTabProps = { uuid: string; core: PlayerCore | undefined };

const WINDOW_OPTIONS = PLAYER_WINDOWS.map((value) => ({ value, label: value }));

export function RequestsTab({ uuid, core }: RequestsTabProps) {
  const { selection, setWindow, toggleForm } = useLookupSelection();
  const [requester, setRequester] = useUrlParam("requester");
  const requesters = useQuery({ ...playerRequestersQuery(uuid, selection), placeholderData: keepPreviousData });
  const names = useMemo(() => core?.names.map((name) => name.name.toLowerCase()) ?? [], [core]);

  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Segmented label="Window" value={selection.window} options={WINDOW_OPTIONS} onChange={setWindow} />
        <span className="text-12 text-subtle">Matches the UUID in both forms, the current name, former names inside their held windows, paths and batch bodies.</span>
      </div>
      <FormChips
        selected={selection.forms}
        counts={requesters.data?.forms ?? null}
        currentName={currentName(core)}
        formerNames={formerNames(core)}
        onToggle={toggleForm}
      />
      {requesters.data !== undefined && <LookupNotices requesters={requesters.data} />}
      <Section
        title="Requesters"
        count={requesters.data === undefined ? undefined : <RequestersCount requesters={requesters.data} />}
        flush
        fetching={requesters.isFetching && requesters.data !== undefined}
        actions={requester === null ? undefined : <ClearRequester onClear={() => setRequester(null)} />}
      >
        {requesters.error !== null && requesters.data === undefined ? (
          <div className="p-3">
            <ErrorState error={requesters.error} onRetry={() => void requesters.refetch()} retrying={requesters.isFetching} />
          </div>
        ) : (
          <RequestersTable groups={requesters.data?.groups ?? []} selectedKey={requester} onSelect={setRequester} loading={requesters.isPending} />
        )}
      </Section>
      <RequestStream uuid={uuid} selection={selection} requester={requester} names={names} />
    </div>
  );
}

function LookupNotices({ requesters }: { requesters: PlayerRequesters }) {
  const { coverage } = useSession();
  return (
    <div className="flex flex-col gap-1.5">
      <CoverageNotices coverage={coverage} features={["subjects"]} from={requesters.from} to={requesters.to} clampedFrom={requesters.clamped_from} />
      <BatchBodyNotice coverage={coverage} requesters={requesters} />
    </div>
  );
}

function BatchBodyNotice({ coverage, requesters }: { coverage: Coverage; requesters: PlayerRequesters }) {
  const { time } = useTimeFormat();
  const since = coverage.body_subjects_from;
  if (since !== null && parseTimestamp(requesters.from).getTime() >= parseTimestamp(since).getTime()) return null;
  const count =
    requesters.uncaptured_batches === null
      ? "the traffic rollups do not cover this window, so their number is unknown"
      : `${formatNumber(requesters.uncaptured_batches)} batch requests in this window`;
  const stamp = since === null ? "now" : time(since, { seconds: false, date: "always", zone: true });
  return (
    <CoverageBanner tone="warning">
      Batch-body lookups before {stamp} were never captured ({count}).
    </CoverageBanner>
  );
}

function RequestersCount({ requesters }: { requesters: PlayerRequesters }) {
  const { time } = useTimeFormat();
  return (
    <>
      {formatNumber(requesters.groups.length)} · counted {time(requesters.to)}
    </>
  );
}

function ClearRequester({ onClear }: { onClear: () => void }) {
  return (
    <Button size="sm" variant="ghost" icon={<X className="size-3" />} onClick={onClear}>
      Show every requester
    </Button>
  );
}
