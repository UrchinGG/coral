import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { apiDelete, apiGet, apiPost, apiPut } from "./client";
import type { DiscordServerDetail, StartedJobResponse, SyncJobsResponse, SyncJobView } from "./types";
import { useAdminMutation } from "./useAdminMutation";

const SERVER_KEY = ["discord", "server"];
const JOBS_KEY = ["discord", "jobs"];
const SERVER_KEYS = [SERVER_KEY, JOBS_KEY];

type JobStreamEvent =
  | { type: "snapshot"; job: SyncJobView }
  | { type: "progress"; processed: number; total: number }
  | { type: "finished"; status: SyncJobView["state"] };

export function useDiscordServer() {
  return useQuery({
    queryKey: SERVER_KEY,
    queryFn: () => apiGet<DiscordServerDetail>("/server"),
  });
}

export function useSyncJobs() {
  return useQuery({
    queryKey: JOBS_KEY,
    queryFn: () => apiGet<SyncJobsResponse>("/server/jobs"),
  });
}

export function useSyncJobEvents(jobs: SyncJobView[] | undefined) {
  const queryClient = useQueryClient();
  const activeIds = (jobs ?? [])
    .filter((job) => job.state === "queued" || job.state === "running")
    .map((job) => job.id);
  const activeKey = activeIds.join(",");

  useEffect(() => {
    if (activeIds.length === 0) return;

    const sources = activeIds.map((jobId) => {
      const source = new EventSource(`/api/server/jobs/${jobId}/events`);
      source.onmessage = (message) => {
        const event = JSON.parse(message.data) as JobStreamEvent;
        if (event.type === "snapshot") {
          replaceJob(queryClient, event.job);
          if (isFinishedState(event.job.state)) source.close();
        } else if (event.type === "progress") {
          patchJob(queryClient, jobId, {
            state: "running",
            processed: event.processed,
            total: event.total,
          });
        } else {
          source.close();
          for (const key of SERVER_KEYS) {
            queryClient.invalidateQueries({ queryKey: key });
          }
        }
      };
      return source;
    });

    return () => sources.forEach((source) => source.close());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeKey, queryClient]);
}

function isFinishedState(state: SyncJobView["state"]): boolean {
  return state === "done" || state === "cancelled" || state === "failed";
}

function replaceJob(queryClient: QueryClient, job: SyncJobView) {
  queryClient.setQueryData<SyncJobsResponse>(JOBS_KEY, (data) =>
    data ? { jobs: data.jobs.map((j) => (j.id === job.id ? job : j)) } : data,
  );
}

function patchJob(queryClient: QueryClient, jobId: number, patch: Partial<SyncJobView>) {
  queryClient.setQueryData<SyncJobsResponse>(JOBS_KEY, (data) =>
    data ? { jobs: data.jobs.map((j) => (j.id === jobId ? { ...j, ...patch } : j)) } : data,
  );
}

export function useSetLinkRole() {
  return useAdminMutation(
    (roleId: string | null) => apiPut<StartedJobResponse>("/server/link-role", { role_id: roleId }),
    {
      successMessage: "Linked role updated",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useSetUnlinkedRole() {
  return useAdminMutation(
    (roleId: string | null) => apiPut<StartedJobResponse>("/server/unlinked-role", { role_id: roleId }),
    {
      successMessage: "Unlinked role updated",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useSetLinkChannel() {
  return useAdminMutation(
    (channelId: string | null) => apiPut<void>("/server/link-channel", { channel_id: channelId }),
    {
      successMessage: "Link channel updated",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useSetNicknameTemplate() {
  return useAdminMutation(
    (template: string | null) => apiPut<StartedJobResponse>("/server/nickname-template", { template }),
    {
      successMessage: "Display name format saved",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useResetNicknames() {
  return useAdminMutation<void, StartedJobResponse>(
    () => apiPost<StartedJobResponse>("/server/nicknames/reset"),
    {
      successMessage: "Nickname reset started",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useAddAutorole() {
  return useAdminMutation(
    (rule: { role_id: string; condition: string }) => apiPost<StartedJobResponse>("/server/rules", rule),
    {
      successMessage: "Autorole rule added",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useUpdateAutorole() {
  return useAdminMutation(
    (rule: { id: number; condition: string }) =>
      apiPut<StartedJobResponse>(`/server/rules/${rule.id}`, { condition: rule.condition }),
    {
      successMessage: "Autorole rule updated",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useRemoveAutorole() {
  return useAdminMutation((ruleId: number) => apiDelete<void>(`/server/rules/${ruleId}`), {
    successMessage: "Autorole rule removed",
    invalidateKeys: SERVER_KEYS,
  });
}

export function useStripRole() {
  return useAdminMutation(
    (roleId: string) => apiPost<StartedJobResponse>(`/server/roles/${roleId}/strip`),
    {
      successMessage: "Role strip started",
      invalidateKeys: SERVER_KEYS,
    },
  );
}

export function useCancelSyncJob() {
  return useAdminMutation((jobId: number) => apiPost<void>(`/server/jobs/${jobId}/cancel`), {
    successMessage: "Cancellation requested",
    invalidateKeys: [JOBS_KEY],
  });
}
