import { useQuery } from "@tanstack/react-query";

import { keys } from "../../api/keys";
import { useAction } from "../../api/mutate";
import { cancelSyncJob, syncJobsQuery } from "../../api/server";
import type { SyncJob, SyncJobState } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { QueryContent } from "../../kit/QueryContent";
import { Section } from "../../kit/Section";
import type { Tone } from "../../kit/tone";
import { formatNumber } from "../../lib/format";
import { ProgressBar } from "../platform/ProgressBar";
import { When } from "../platform/When";
import { isActiveJob, useJobEvents } from "./useJobEvents";

const STATE_TONE: Record<SyncJobState, Tone> = { queued: "neutral", running: "info", done: "ok", cancelled: "warning", failed: "danger" };
const STATE_LABEL: Record<SyncJobState, string> = { queued: "Queued", running: "Running", done: "Done", cancelled: "Cancelled", failed: "Failed" };

export function SyncJobs() {
  const jobs = useQuery(syncJobsQuery());
  useJobEvents(jobs.data?.jobs);
  return (
    <Section title="Bulk updates" count={jobs.data?.jobs.length} flush fetching={jobs.isFetching && jobs.data !== undefined}>
      <QueryContent query={jobs} isEmpty={(data) => data.jobs.length === 0} empty="No bulk updates have run on this server.">
        {(data) => (
          <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {data.jobs.map((job) => (
              <JobRow key={job.id} job={job} />
            ))}
          </ul>
        )}
      </QueryContent>
    </Section>
  );
}

export function ActiveJobs() {
  const jobs = useQuery(syncJobsQuery());
  useJobEvents(jobs.data?.jobs);
  const active = jobs.data?.jobs.filter(isActiveJob) ?? [];
  if (active.length === 0) return null;
  return (
    <Section title="Running now" count={active.length} flush>
      <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
        {active.map((job) => (
          <JobRow key={job.id} job={job} />
        ))}
      </ul>
    </Section>
  );
}

function JobRow({ job }: { job: SyncJob }) {
  const cancel = useAction<void, void>({
    run: () => cancelSyncJob(job.id),
    pending: () => `Asking “${job.label}” to stop…`,
    done: () => "Cancellation requested",
    invalidates: [keys.server.jobs()],
  });
  return (
    <li className="flex flex-wrap items-center gap-4 px-3.5 py-2.5">
      <div className="flex min-w-0 flex-[1_1_20rem] flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-13 wrap-anywhere">{job.label}</span>
          <Pill tone={STATE_TONE[job.state]}>{STATE_LABEL[job.state]}</Pill>
          <span className="font-mono text-11 text-subtle">{job.kind}</span>
        </div>
        <JobProgress job={job} />
        {job.error !== null && <span className="text-12 text-danger wrap-anywhere">{job.error}</span>}
      </div>
      {isActiveJob(job) && (
        <ConfirmButton variant="danger" confirmLabel="Stop job" pending={cancel.isPending} onConfirm={() => cancel.mutate()}>
          Cancel
        </ConfirmButton>
      )}
    </li>
  );
}

function JobProgress({ job }: { job: SyncJob }) {
  if (job.state === "queued") return <span className="text-12 text-muted">Waiting for a running job to finish…</span>;
  if (job.state === "running") return <ProgressBar done={job.processed} total={job.total} unit="members" />;
  return (
    <span className="text-12 text-muted">
      {formatNumber(job.processed)} of {formatNumber(job.total)} members · started <When at={job.started_at} /> · finished <When at={job.finished_at} empty="—" />
    </span>
  );
}
