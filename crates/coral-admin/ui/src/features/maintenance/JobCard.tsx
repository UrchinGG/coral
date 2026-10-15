import { useState } from "react";

import { keys } from "../../api/keys";
import { isActiveJob, recomputeRollups, startJob, type StartableJob } from "../../api/maintenance";
import { useAction } from "../../api/mutate";
import type { JobState, JobStatus, Maintenance, MaintenanceJob } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { ConfirmButton } from "../../kit/ConfirmButton";
import { Input } from "../../kit/Input";
import { KeyValue, type KeyValueItem } from "../../kit/KeyValue";
import { Section } from "../../kit/Section";
import type { Tone } from "../../kit/tone";
import { formatNumber, plural } from "../../lib/format";
import { fromInputValue, useTimeFormat, zoneLabel } from "../../lib/time";
import { ProgressBar } from "../platform/ProgressBar";
import { When } from "../platform/When";
import { ROLLUP_JOB, startableJob } from "./jobs";
import { RollupPasses } from "./RollupPasses";

type Pending = Maintenance["pending"];

type JobCardProps = { job: MaintenanceJob; description: string | null; pending: Pending; fetching: boolean };

const STATE_TONE: Record<JobState, Tone> = { running: "info", waiting: "warning", succeeded: "ok", failed: "danger", interrupted: "warning" };
const DISCORD_NAMES_PER_SECOND = 2;
const SECONDS_PER_MINUTE = 60;

export function JobCard({ job, description, pending, fetching }: JobCardProps) {
  const startable = startableJob(job.name);
  return (
    <Section
      title={<span className="font-mono">{job.name}</span>}
      actions={job.status === null ? <Pill>never run</Pill> : <Pill tone={STATE_TONE[job.status.state]}>{job.status.state}</Pill>}
      fetching={fetching}
      className={job.name === ROLLUP_JOB ? "xl:col-span-2" : undefined}
    >
      <div className="flex flex-col gap-3">
        {description !== null && <p className="m-0 text-12 text-muted">{description}</p>}
        <JobStatusView job={job} />
        {startable !== null && <StartControls job={startable} active={isActiveJob(job.status)} pending={pending} />}
        {job.name === "recompute" && <RecomputeForm active={isActiveJob(job.status)} />}
      </div>
    </Section>
  );
}

function JobStatusView({ job }: { job: MaintenanceJob }) {
  const { status } = job;
  if (status === null) return null;
  const items: KeyValueItem[] = [
    { key: "started", label: "Started", value: <When at={status.started_at} /> },
    { key: "updated", label: "Last report", value: <When at={status.updated_at} /> },
    { key: "finished", label: "Finished", value: <When at={status.finished_at} empty="not yet" /> },
    { key: "instance", label: "Instance", value: <span className="font-mono text-12">{status.instance}</span> },
  ];
  return (
    <div className="flex flex-col gap-2.5">
      <KeyValue items={items} />
      {job.name === ROLLUP_JOB ? <RollupPasses progress={status.progress} /> : <Progress progress={status.progress} />}
      {status.error !== null && <span className="font-mono text-12 text-danger wrap-anywhere">{status.error}</span>}
    </div>
  );
}

function Progress({ progress }: { progress: unknown }) {
  if (!isRecord(progress)) return null;
  const { done, total, unit, ...rest } = progress;
  const bar = typeof done === "number" && typeof total === "number" ? <ProgressBar done={done} total={total} unit={typeof unit === "string" ? unit : ""} /> : null;
  const facts = Object.entries(rest);
  return (
    <div className="flex flex-col gap-2">
      {bar}
      {facts.length > 0 && <KeyValue items={facts.map(([key, value]) => ({ key, label: key, value: <FactValue value={value} /> }))} />}
    </div>
  );
}

function FactValue({ value }: { value: unknown }) {
  if (typeof value === "number") return <span className="font-mono text-12">{Number.isInteger(value) ? formatNumber(value) : value.toFixed(4)}</span>;
  if (typeof value === "string") return <span className="font-mono text-12 wrap-anywhere">{value}</span>;
  return <pre className="m-0 max-h-48 overflow-auto rounded bg-bg p-2 font-mono text-11 whitespace-pre-wrap">{JSON.stringify(value, null, 2)}</pre>;
}

function StartControls({ job, active, pending }: { job: StartableJob; active: boolean; pending: Pending }) {
  const start = useAction<void, JobStatus>({
    run: () => startJob(job),
    pending: () => `Starting ${job}…`,
    done: () => `${job} started`,
    invalidates: [keys.maintenance()],
  });
  const { time } = useTimeFormat();
  const work = pendingWork(job, pending, (iso) => time(iso, { seconds: false, date: "always", zone: true }));
  return (
    <div className="flex flex-col gap-2 border-t border-border pt-3">
      <div className="text-12">{work.summary}</div>
      {work.details.length > 0 && (
        <ul className="m-0 flex max-h-40 list-none flex-col gap-0.5 overflow-y-auto p-0 font-mono text-11 text-muted">
          {work.details.map((detail) => (
            <li key={detail} className="wrap-anywhere">
              {detail}
            </li>
          ))}
        </ul>
      )}
      <span>
        <ConfirmButton confirmLabel={`Start ${job}`} disabled={active || !work.any} pending={start.isPending} onConfirm={() => start.mutate()}>
          {active ? "Running" : "Start"}
        </ConfirmButton>
      </span>
    </div>
  );
}

function RecomputeForm({ active }: { active: boolean }) {
  const { mode, time } = useTimeFormat();
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const range = { from: fromInputValue(from, mode), to: fromInputValue(to, mode) };
  const recompute = useAction({
    run: ({ from: start, to: end }: { from: string; to: string }) => recomputeRollups(start, end),
    pending: () => "Starting the recompute…",
    done: (status) => `recompute started at ${time(status.started_at, { seconds: false })}`,
    invalidates: [keys.maintenance()],
  });
  const ready = range.from !== null && range.to !== null && range.from < range.to;
  return (
    <div className="flex flex-wrap items-end gap-2 border-t border-border pt-3">
      <Input label={`From (${zoneLabel(mode)})`} type="datetime-local" value={from} onChange={(event) => setFrom(event.target.value)} />
      <Input label={`To (${zoneLabel(mode)})`} type="datetime-local" value={to} onChange={(event) => setTo(event.target.value)} />
      <ConfirmButton
        confirmLabel="Replace stored rollups"
        disabled={active || !ready}
        pending={recompute.isPending}
        onConfirm={() => range.from !== null && range.to !== null && recompute.mutate({ from: range.from, to: range.to })}
      >
        Recompute range
      </ConfirmButton>
    </div>
  );
}

function pendingWork(job: StartableJob, pending: Pending, format: (iso: string) => string): { any: boolean; summary: string; details: string[] } {
  switch (job) {
    case "subjects":
      return {
        any: pending.subjects.spans.length > 0,
        summary: pending.subjects.spans.length === 0 ? "Every retained request already has its subjects." : `${plural(pending.subjects.spans.length, "range")} to backfill:`,
        details: pending.subjects.spans.map((span) => `${span.kind === "gap" ? "gap" : "before coverage"}: ${format(span.from)} → ${format(span.to)}`),
      };
    case "discord-names":
      return {
        any: pending.discord_names > 0,
        summary: pending.discord_names === 0 ? "Every member's Discord name is cached." : `${plural(pending.discord_names, "member")} without a cached Discord name, about ${plural(Math.ceil(pending.discord_names / DISCORD_NAMES_PER_SECOND / SECONDS_PER_MINUTE), "minute")} of fetching.`,
        details: [],
      };
    case "index-catchup":
      return {
        any: pending.indexes.length > 0,
        summary: pending.indexes.length === 0 ? "Every partition has every index." : `${plural(pending.indexes.length, "partition index", "partition indexes")} missing:`,
        details: pending.indexes,
      };
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
