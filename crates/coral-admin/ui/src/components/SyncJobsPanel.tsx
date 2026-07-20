import { useCancelSyncJob, useSyncJobEvents, useSyncJobs } from "../api/discord";
import type { SyncJobView } from "../api/types";
import { fmtDate, fmtNum } from "../format";
import { Badge } from "./Badge";
import { ConfirmButton } from "./ConfirmButton";
import { Panel } from "./Panel";

const STATE_TONES = { queued: "default", running: "accent", done: "ok", cancelled: "warning", failed: "danger" } as const;
const STATE_LABELS = { queued: "Queued", running: "Running", done: "Done", cancelled: "Cancelled", failed: "Failed" } as const;

export function SyncJobsPanel() {
  const jobs = useSyncJobs();
  const cancel = useCancelSyncJob();
  useSyncJobEvents(jobs.data?.jobs);

  const rows = jobs.data?.jobs ?? [];
  if (rows.length === 0) return null;

  return (
    <Panel
      title="Bulk updates"
      description="Member-wide updates run in the background and can take a while on large servers."
    >
      <div className="flex flex-col divide-y divide-white/5">
        {rows.map((job) => (
          <JobRow key={job.id} job={job} onCancel={() => cancel.mutate(job.id)} cancelPending={cancel.isPending} />
        ))}
      </div>
    </Panel>
  );
}

function JobRow({ job, onCancel, cancelPending }: { job: SyncJobView; onCancel: () => void; cancelPending: boolean }) {
  const fraction = job.total > 0 ? job.processed / job.total : 0;
  const active = job.state === "running" || job.state === "queued";

  return (
    <div className="flex items-center gap-4 py-2.5 first:pt-0 last:pb-0">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm text-gray-200">{job.label}</span>
          <Badge label={STATE_LABELS[job.state]} tone={STATE_TONES[job.state]} />
        </div>
        {job.state === "running" ? (
          <div className="mt-1.5 flex items-center gap-2">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-white/8">
              <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${fraction * 100}%` }} />
            </div>
            <span className="text-xs whitespace-nowrap text-gray-500">
              {fmtNum(job.processed)} / {fmtNum(job.total)} members
            </span>
          </div>
        ) : job.state === "queued" ? (
          <div className="mt-0.5 text-xs text-gray-500">Waiting for a running job to finish…</div>
        ) : (
          <div className="mt-0.5 text-xs text-gray-500">
            {fmtNum(job.processed)} of {fmtNum(job.total)} members · finished {fmtDate(job.finished_at)}
          </div>
        )}
      </div>
      {active && (
        <ConfirmButton label="Cancel" confirmLabel="Stop job" tone="danger" onConfirm={onCancel} pending={cancelPending} />
      )}
    </div>
  );
}
