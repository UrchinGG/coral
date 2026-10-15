import { useState } from "react";

import type { SeriesLine, ViewAction, ViewSection } from "../../api/types/extension";
import { Button } from "../../kit/Button";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { Section } from "../../kit/Section";
import { Sparkline } from "../../kit/Sparkline";
import { CoverageBanner } from "../../kit/States";
import { Table, type Column } from "../../kit/Table";
import { formatNumber } from "../../lib/format";
import type { ActionRun } from "./useExtensionAction";
import { ViewActionButton } from "./ViewActionButton";
import { initialValues, missingRequired } from "./field-values";
import { VIEW_TONE } from "./tones";
import { ViewCellValue } from "./ViewCells";
import { ViewFields } from "./ViewFields";

type SectionOf<T extends ViewSection["type"]> = Extract<ViewSection, { type: T }>;

type Runner = { pending: ViewAction | null; run: (run: ActionRun) => Promise<unknown> };

type ViewRow = SectionOf<"table">["rows"][number];

const TONE_COLOR: Record<string, string> = {
  neutral: "var(--color-muted)",
  subtle: "var(--color-subtle)",
  ok: "var(--color-ok)",
  info: "var(--color-info)",
  warning: "var(--color-warning)",
  limited: "var(--color-limited)",
  danger: "var(--color-danger)",
};

export function ViewSectionBlock({ section, runner }: { section: ViewSection; runner: Runner }) {
  switch (section.type) {
    case "notice":
      return <NoticeSection section={section} runner={runner} />;
    case "stats":
      return <StatsSection section={section} />;
    case "table":
      return <TableSection section={section} runner={runner} />;
    case "form":
      return <FormSection key={section.id} section={section} runner={runner} />;
    case "series":
      return <SeriesSection section={section} />;
  }
}

function NoticeSection({ section, runner }: { section: SectionOf<"notice">; runner: Runner }) {
  return (
    <CoverageBanner tone={section.level === "info" ? "info" : "warning"}>
      <span className="flex flex-wrap items-center gap-2">
        <span className={section.level === "danger" ? "text-danger" : undefined}>{section.text}</span>
        {section.actions.map((action) => (
          <ViewActionButton key={actionKey(action)} action={action} pending={runner.pending === action} onRun={runner.run} />
        ))}
      </span>
    </CoverageBanner>
  );
}

function StatsSection({ section }: { section: SectionOf<"stats"> }) {
  return (
    <KpiStrip>
      {section.items.map((item) => (
        <KpiTile key={item.label} label={item.label} value={item.value} sub={item.detail ?? undefined} />
      ))}
    </KpiStrip>
  );
}

function TableSection({ section, runner }: { section: SectionOf<"table">; runner: Runner }) {
  const withActions = section.rows.some((row) => row.actions.length > 0);
  const columns: Column<ViewRow>[] = section.columns.map((column) => ({
    id: column.key,
    header: column.label,
    numeric: column.numeric,
    cell: (row) => <ViewCellValue cell={row.cells[column.key]} />,
  }));
  if (withActions) {
    columns.push({
      id: "actions",
      header: "",
      cell: (row) => (
        <span className="flex flex-wrap justify-end gap-1.5">
          {row.actions.map((action) => (
            <ViewActionButton key={actionKey(action)} action={action} pending={runner.pending === action} onRun={runner.run} />
          ))}
        </span>
      ),
    });
  }
  return (
    <Section title={section.title} count={section.rows.length} flush>
      <Table label={section.title} columns={columns} rows={section.rows} rowKey={(row) => row.key} empty="—" />
    </Section>
  );
}

function FormSection({ section, runner }: { section: SectionOf<"form">; runner: Runner }) {
  const [values, setValues] = useState(() => initialValues(section.fields));
  const blocked = missingRequired(section.fields, values);
  return (
    <Section title={section.title}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void runner.run({ action: section.submit, values }).then(() => setValues(initialValues(section.fields)), () => undefined);
        }}
        className="flex flex-col gap-3"
      >
        <ViewFields fields={section.fields} values={values} onChange={setValues} />
        <span>
          <Button type="submit" variant={section.submit.danger ? "danger" : "primary"} disabled={blocked} pending={runner.pending === section.submit}>
            {section.submit.label}
          </Button>
        </span>
      </form>
    </Section>
  );
}

function SeriesSection({ section }: { section: SectionOf<"series"> }) {
  return (
    <Section title={section.title}>
      <div className="flex flex-col gap-3">
        {section.lines.map((line) => (
          <SeriesRow key={line.label} line={line} unit={section.unit} />
        ))}
      </div>
    </Section>
  );
}

function SeriesRow({ line, unit }: { line: SeriesLine; unit: string | null }) {
  const values = line.points.map((point) => point.value);
  const latest = values.at(-1);
  return (
    <div className="grid grid-cols-[minmax(120px,200px)_minmax(0,1fr)_auto] items-center gap-3 text-12">
      <span className="wrap-anywhere">{line.label}</span>
      <Sparkline values={values} color={TONE_COLOR[VIEW_TONE[line.tone]] ?? TONE_COLOR.neutral} label={line.label} />
      <span className="font-mono whitespace-nowrap">
        {latest === undefined ? "—" : formatNumber(latest)}
        {unit !== null && latest !== undefined && <span className="ml-1 text-subtle">{unit}</span>}
      </span>
    </div>
  );
}

function actionKey(action: ViewAction): string {
  return `${action.action}:${JSON.stringify(action.input)}`;
}
