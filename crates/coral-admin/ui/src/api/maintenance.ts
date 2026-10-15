import { queryOptions } from "@tanstack/react-query";

import { postJson } from "./http";
import { keys } from "./keys";
import { getQuery, STALE_LIVE_MS } from "./queryClient";
import type { JobState, JobStatus, Maintenance } from "./types/platform";

export const MAINTENANCE_POLL_MS = 5_000;

export type StartableJob = "subjects" | "discord-names" | "index-catchup";

const ACTIVE_STATES: readonly JobState[] = ["running", "waiting"];

export function maintenanceQuery() {
  return queryOptions({
    queryKey: keys.maintenance(),
    queryFn: getQuery<Maintenance>("/maintenance"),
    staleTime: STALE_LIVE_MS,
    refetchInterval: (query) => (hasActiveJob(query.state.data) ? MAINTENANCE_POLL_MS : false),
    refetchIntervalInBackground: false,
  });
}

export function isActiveJob(status: JobStatus | null): boolean {
  return status !== null && ACTIVE_STATES.includes(status.state);
}

export function startJob(job: StartableJob): Promise<JobStatus> {
  return postJson<JobStatus>(`/maintenance/${job}`, {});
}

export function recomputeRollups(from: string, to: string): Promise<JobStatus> {
  return postJson<JobStatus>("/maintenance/recompute", { from, to });
}

function hasActiveJob(data: Maintenance | undefined): boolean {
  return data?.jobs.some((job) => isActiveJob(job.status)) ?? false;
}
