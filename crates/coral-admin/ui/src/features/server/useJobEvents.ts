import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { keys } from "../../api/keys";
import { syncJobEventsUrl } from "../../api/server";
import type { SyncJob, SyncJobEvent, SyncJobs, SyncJobState } from "../../api/types/platform";

const ACTIVE_STATES: readonly SyncJobState[] = ["queued", "running"];

export function isActiveJob(job: SyncJob): boolean {
  return ACTIVE_STATES.includes(job.state);
}

export function useJobEvents(jobs: readonly SyncJob[] | undefined): void {
  const queryClient = useQueryClient();
  const activeKey = (jobs ?? [])
    .filter(isActiveJob)
    .map((job) => job.id)
    .join(",");

  useEffect(() => {
    if (activeKey === "") return;
    const sources = activeKey.split(",").map((id) => followJob(queryClient, Number(id)));
    return () => sources.forEach((source) => source.close());
  }, [activeKey, queryClient]);
}

function followJob(queryClient: QueryClient, jobId: number): EventSource {
  const source = new EventSource(syncJobEventsUrl(jobId));
  source.onmessage = (message: MessageEvent<string>) => {
    const event = JSON.parse(message.data) as SyncJobEvent;
    applyEvent(queryClient, jobId, event, () => source.close());
  };
  source.onerror = () => {
    if (source.readyState === EventSource.CLOSED) void queryClient.invalidateQueries({ queryKey: keys.server.jobs() });
  };
  return source;
}

function applyEvent(queryClient: QueryClient, jobId: number, event: SyncJobEvent, close: () => void): void {
  switch (event.type) {
    case "snapshot":
      patchJob(queryClient, jobId, event.job);
      if (!isActiveJob(event.job)) close();
      return;
    case "progress":
      patchJob(queryClient, jobId, { state: "running", processed: event.processed, total: event.total });
      return;
    case "finished":
      close();
      patchJob(queryClient, jobId, { state: event.status });
      void queryClient.invalidateQueries({ queryKey: keys.server.all() });
      return;
    case "failed":
      close();
      patchJob(queryClient, jobId, { error: event.message });
      return;
  }
}

function patchJob(queryClient: QueryClient, jobId: number, patch: Partial<SyncJob>): void {
  queryClient.setQueryData<SyncJobs>(keys.server.jobs(), (data) =>
    data === undefined ? data : { jobs: data.jobs.map((job) => (job.id === jobId ? { ...job, ...patch } : job)) },
  );
}
