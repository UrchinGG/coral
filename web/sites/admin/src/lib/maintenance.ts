import { FORMATS } from "@/lib/format";
import { moment } from "@/lib/time";

export interface JobStatus {
  state: string;
  started_at: string;
  finished_at: string | null;
  progress: unknown;
  error: string | null;
}

export interface RollupState {
  log_from: string | null;
  watermarks: { name: string; at: string }[];
  dirty_ranges: number;
}

interface Counted {
  done: number;
  total: number;
  unit: string;
}

interface Recomputed {
  from: string;
  to: string;
  done_until: string;
}

export const ACTIVE_STATES = ["running", "waiting"];
const ROLLUP_JOB = "rollups";
const SECOND_MS = 1_000;
const MINUTE_S = 60;
const HOUR_S = 3_600;
const UNITS: Record<string, [string, string]> = {
  slices: ["day", "days"],
  ids: ["name", "names"],
  indexes: ["index", "indexes"],
};

const isActive = (status: JobStatus) => ACTIVE_STATES.includes(status.state);

const percent = (done: number, total: number) =>
  total ? Math.floor((done / total) * 100) : 0;

export function lastResult(
  name: string,
  status: JobStatus,
  rollups: RollupState,
) {
  if (status.error) return status.error;
  if (name === ROLLUP_JOB) return rollupResult(rollups);
  const progress = status.progress as Partial<Counted & Recomputed> | null;
  if (progress?.total !== undefined)
    return countedResult(progress as Counted, status);
  if (progress?.done_until !== undefined)
    return recomputeResult(progress as Recomputed, status);
  return isActive(status) ? "Starting" : `Nothing to do, ${took(status)}`;
}

function countedResult(progress: Counted, status: JobStatus) {
  const [one, many] = UNITS[progress.unit] ?? [progress.unit, progress.unit];
  if (isActive(status))
    return `${percent(progress.done, progress.total)}% · ${one} ${FORMATS.number(progress.done)} of ${FORMATS.number(progress.total)}`;
  return `${FORMATS.number(progress.done)} ${progress.done === 1 ? one : many}, ${took(status)}`;
}

function recomputeResult(progress: Recomputed, status: JobStatus) {
  const [from, to, until] = [
    progress.from,
    progress.to,
    progress.done_until,
  ].map(Date.parse);
  if (isActive(status))
    return `${percent(until - from, to - from)}% · through ${moment(progress.done_until)}`;
  return `${moment(progress.from)} to ${moment(progress.to)}, ${took(status)}`;
}

function rollupResult(rollups: RollupState) {
  const oldest = rollups.watermarks.map((mark) => mark.at).sort()[0];
  const rolled = oldest
    ? `Rolled up to ${moment(oldest)}`
    : "Nothing rolled up yet";
  return rollups.dirty_ranges
    ? `${rolled} · ${FORMATS.number(rollups.dirty_ranges)} ranges to redo`
    : rolled;
}

function took(status: JobStatus) {
  const end = status.finished_at ? Date.parse(status.finished_at) : Date.now();
  const seconds = Math.round((end - Date.parse(status.started_at)) / SECOND_MS);
  if (seconds < MINUTE_S) return `${seconds} s`;
  if (seconds < HOUR_S) return `${Math.round(seconds / MINUTE_S)} min`;
  const minutes = Math.round((seconds % HOUR_S) / MINUTE_S);
  return `${Math.floor(seconds / HOUR_S)} h ${minutes} min`;
}

export const pollsFor = (jobs: { name: string; status: JobStatus | null }[]) =>
  jobs.some(
    (job) =>
      job.name !== ROLLUP_JOB && job.status !== null && isActive(job.status),
  );
