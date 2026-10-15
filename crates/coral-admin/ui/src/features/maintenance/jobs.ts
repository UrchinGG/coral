import type { StartableJob } from "../../api/maintenance";

export const JOB_DESCRIPTIONS: Readonly<Record<string, string>> = {
  rollups: "Rolls traffic, callers, network and top players out of the request log. It runs by itself and restarts after a crash.",
  recompute: "Reruns the traffic and caller rollups over a time range, replacing what is stored there.",
  subjects: "Extracts the players each request named for days before subject logging began and for recorded gaps, oldest day first, then moves subject coverage back.",
  "discord-names": "Fetches the Discord names of members with none cached, through the shared Discord name worker at 2 per second.",
  "index-catchup": "Builds any index a request log partition is missing and attaches it to its parent index.",
};

export const ROLLUP_JOB = "rollups";

export const STARTABLE_JOBS: readonly StartableJob[] = ["subjects", "discord-names", "index-catchup"];

export function startableJob(name: string): StartableJob | null {
  return STARTABLE_JOBS.find((job) => job === name) ?? null;
}
