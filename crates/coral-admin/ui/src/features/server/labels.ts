import type { StartedJob } from "../../api/types/platform";
import { formatNumber } from "../../lib/format";

export function membersPhrase(memberCount: number | undefined): string {
  return memberCount === undefined ? "every member" : `${formatNumber(memberCount)} members`;
}

export function nicknamesPhrase(memberCount: number | undefined): string {
  return memberCount === undefined ? "every nickname" : `${formatNumber(memberCount)} nicknames`;
}

export function startedText(started: StartedJob, done: string): string {
  return started.job === null ? done : `${done}; started “${started.job.label}”`;
}
