import { queryOptions } from "@tanstack/react-query";

import { deleteJson, getJson, postJson, putJson } from "./http";
import { keys } from "./keys";
import { configQuery, getQuery, STALE_LIST_MS } from "./queryClient";
import type { Directory, Population, ServerConfig, StartedJob, SyncJobs } from "./types/platform";

export const SERVER_INVALIDATES = [keys.server.config(), keys.server.jobs()] as const;

export function serverConfigQuery() {
  return queryOptions({ queryKey: keys.server.config(), queryFn: getQuery<ServerConfig>("/server"), ...configQuery });
}

export function directoryQuery() {
  return queryOptions({ queryKey: keys.server.directory(), queryFn: getQuery<Directory>("/server/directory"), ...configQuery });
}

export function populationQuery() {
  return queryOptions({ queryKey: keys.server.population(), queryFn: getQuery<Population>("/server/population"), ...configQuery });
}

export function syncJobsQuery() {
  return queryOptions({ queryKey: keys.server.jobs(), queryFn: getQuery<SyncJobs>("/server/jobs"), staleTime: STALE_LIST_MS });
}

export function refreshDirectory(): Promise<Directory> {
  return getJson<Directory>("/server/directory?refresh=true");
}

export function refreshPopulation(): Promise<Population> {
  return getJson<Population>("/server/population?refresh=true");
}

export function setConfigRole(field: "link-role" | "unlinked-role", roleId: string | null): Promise<StartedJob> {
  return putJson<StartedJob>(`/server/${field}`, { role_id: roleId });
}

export function setLinkChannel(channelId: string | null): Promise<void> {
  return putJson<void>("/server/link-channel", { channel_id: channelId });
}

export function setNicknameTemplate(template: string | null): Promise<StartedJob> {
  return putJson<StartedJob>("/server/nickname-template", { template });
}

export function resetNicknames(): Promise<StartedJob> {
  return postJson<StartedJob>("/server/nicknames/reset", {});
}

export function addAutorole(roleId: string, condition: string): Promise<StartedJob> {
  return postJson<StartedJob>("/server/rules", { role_id: roleId, condition });
}

export function updateAutorole(ruleId: number, condition: string): Promise<StartedJob> {
  return putJson<StartedJob>(`/server/rules/${ruleId}`, { condition });
}

export function removeAutorole(ruleId: number): Promise<void> {
  return deleteJson<void>(`/server/rules/${ruleId}`);
}

export function stripRole(roleId: string): Promise<StartedJob> {
  return postJson<StartedJob>(`/server/roles/${roleId}/strip`, {});
}

export function cancelSyncJob(jobId: number): Promise<void> {
  return postJson<void>(`/server/jobs/${jobId}/cancel`, {});
}

export function syncJobEventsUrl(jobId: number): string {
  return `/api/server/jobs/${jobId}/events`;
}
