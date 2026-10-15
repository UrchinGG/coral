import type { PassOutcome, PassReport } from "../../api/types/platform";
import { Pill } from "../../kit/Badges";
import { Table, type Column } from "../../kit/Table";
import type { Tone } from "../../kit/tone";
import { formatMs, formatNumber } from "../../lib/format";
import { When } from "../platform/When";

type PassRow = PassReport & { pass: string };

const OUTCOME_TONE: Record<PassOutcome, Tone> = { done: "ok", behind: "warning", idle: "neutral", locked: "info", waiting: "warning", failed: "danger" };
const OUTCOMES = Object.keys(OUTCOME_TONE);

const PASS_COLUMNS: readonly Column<PassRow>[] = [
  { id: "pass", header: "Pass", cell: (row) => <span className="font-mono text-12">{row.pass}</span> },
  { id: "outcome", header: "Outcome", cell: (row) => <Pill tone={OUTCOME_TONE[row.outcome]}>{row.outcome}</Pill> },
  { id: "covered", header: "Covered", cell: (row) => <Covered row={row} />, priority: 2 },
  { id: "written", header: "Written", cell: (row) => formatNumber(row.written), numeric: true, priority: 2 },
  { id: "removed", header: "Removed", cell: (row) => formatNumber(row.removed), numeric: true, priority: 3 },
  { id: "took", header: "Took", cell: (row) => formatMs(row.duration_ms), numeric: true, priority: 3 },
  { id: "at", header: "Ran", cell: (row) => <When at={row.at} /> },
];

export function RollupPasses({ progress }: { progress: unknown }) {
  const rows = passRows(progress);
  if (rows === null) return null;
  const failures = rows.filter((row) => row.error !== null);
  return (
    <div className="flex flex-col gap-2">
      <Table label="Rollup passes" columns={PASS_COLUMNS} rows={rows} rowKey={(row) => row.pass} empty="No pass has reported yet." />
      {failures.map((row) => (
        <span key={row.pass} className="font-mono text-12 text-danger wrap-anywhere">
          {row.pass}: {row.error}
        </span>
      ))}
    </div>
  );
}

function Covered({ row }: { row: PassRow }) {
  if (row.from === null || row.to === null) return <span className="text-subtle">nothing</span>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      <When at={row.from} /> → <When at={row.to} />
    </span>
  );
}

function passRows(progress: unknown): PassRow[] | null {
  if (typeof progress !== "object" || progress === null) return null;
  const rows: PassRow[] = [];
  for (const [pass, report] of Object.entries(progress)) {
    if (!isPassReport(report)) return null;
    rows.push({ ...report, pass });
  }
  return rows;
}

function isPassReport(value: unknown): value is PassReport {
  return typeof value === "object" && value !== null && "outcome" in value && OUTCOMES.includes(String(value.outcome)) && "at" in value && typeof value.at === "string";
}
