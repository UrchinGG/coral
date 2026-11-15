import { call, load } from "@/lib/api";
import { el } from "@/lib/dom";
import { FORMATS, titleCase } from "@/lib/format";
import { smallButton, type Job } from "@/lib/server/common";
import { ago } from "@/lib/time";

const POLL_MS = 3_000;
const ACTIVE = ["queued", "running"];
const STATE_CLASS: Record<Job["state"], string> = {
  queued: "dim",
  running: "ok",
  done: "dim",
  cancelled: "warn",
  failed: "bad",
};

let poll: ReturnType<typeof setTimeout> | undefined;

const isActive = (job: Job) => ACTIVE.includes(job.state);

export async function watchJobs(
  base: string,
  container: HTMLElement,
  onlyActive: boolean,
  finished: () => void,
  wasActive = false,
) {
  clearTimeout(poll);
  const { jobs } = await load<{ jobs: Job[] }>(`${base}/jobs`);
  const again = () => watchJobs(base, container, onlyActive, finished, true);
  const cancel = (job: Job) => cancelJob(base, job, again);
  container.replaceChildren(
    ...(onlyActive
      ? jobs.filter(isActive).map((job) => runningCard(job, cancel))
      : jobList(jobs, cancel)),
  );
  const active = jobs.some(isActive);
  if (wasActive && !active) finished();
  if (active) poll = setTimeout(again, POLL_MS);
}

async function cancelJob(base: string, job: Job, reload: () => void) {
  if (!confirm(`Stop "${job.label}"?`)) return;
  await call(`${base}/jobs/${job.id}/cancel`, "POST", {});
  reload();
}

function runningCard(job: Job, cancel: (job: Job) => void) {
  const bar = el("span");
  bar.style.width = `${job.total ? (job.processed / job.total) * 100 : 0}%`;
  const counted =
    job.state === "queued"
      ? "Waiting for a running job"
      : `${FORMATS.number(job.processed)} of ${FORMATS.number(job.total)} members · started ${ago(job.started_at)}`;
  return el(
    "div",
    { className: "sync-running" },
    el(
      "span",
      { className: "sync-running-text" },
      el("b", { className: "strong" }, job.label),
      el("span", { className: "muted" }, counted),
    ),
    el("span", { className: "meter ok" }, bar),
    smallButton("Cancel", () => cancel(job), "button small danger"),
  );
}

function jobList(jobs: Job[], cancel: (job: Job) => void) {
  if (!jobs.length)
    return [
      el(
        "p",
        { className: "muted" },
        "No bulk updates have run on this server.",
      ),
    ];
  return [
    el(
      "div",
      { className: "stack" },
      el("h2", { className: "group-title" }, "Bulk Updates"),
      el(
        "div",
        { className: "table" },
        ...jobs.map((job) => jobRow(job, cancel)),
      ),
    ),
  ];
}

function jobRow(job: Job, cancel: (job: Job) => void) {
  return el(
    "div",
    { className: "table-row sync-job" },
    el(
      "span",
      {},
      el("b", { className: "strong" }, job.label),
      el("span", { className: "mono dim block" }, job.kind),
      ...(job.error ? [el("span", { className: "bad block" }, job.error)] : []),
    ),
    el("span", { className: STATE_CLASS[job.state] }, titleCase(job.state)),
    progress(job),
    isActive(job)
      ? smallButton("Cancel", () => cancel(job), "button small danger")
      : el("span"),
  );
}

function progress(job: Job) {
  if (job.state === "queued")
    return el("span", { className: "dim" }, "Waiting for a running job");
  const counted = `${FORMATS.number(job.processed)} of ${FORMATS.number(job.total)} members`;
  if (job.state !== "running")
    return el(
      "span",
      { className: "dim" },
      `${counted} · ${job.finished_at ? `finished ${ago(job.finished_at)}` : `started ${ago(job.started_at)}`}`,
    );
  const bar = el("span");
  bar.style.width = `${job.total ? (job.processed / job.total) * 100 : 0}%`;
  return el(
    "span",
    { className: "meter-cell" },
    el("span", { className: "meter ok" }, bar),
    el("span", { className: "dim" }, counted),
  );
}
