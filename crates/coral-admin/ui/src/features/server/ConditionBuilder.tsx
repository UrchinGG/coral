import { X } from "lucide-react";
import { useId } from "react";

import { Button, IconButton } from "../../kit/Button";
import { CONTROL } from "../../kit/control-styles";
import { Select } from "../../kit/Select";
import { cx } from "../../lib/cx";
import { COMPARE_OPS, type BuilderRow } from "./condition";

type ConditionBuilderProps = { rows: readonly BuilderRow[]; onChange: (rows: BuilderRow[]) => void; fields: readonly string[] };

const OP_OPTIONS = COMPARE_OPS.map((op) => ({ value: op, label: op }));
const JOINER_OPTIONS = [
  { value: "and", label: "and" },
  { value: "or", label: "or" },
] as const;
export function ConditionBuilder({ rows, onChange, fields }: ConditionBuilderProps) {
  const listId = useId();
  const setRow = (index: number, patch: Partial<BuilderRow>) => onChange(rows.map((row, at) => (at === index ? { ...row, ...patch } : row)));

  return (
    <div className="flex flex-col gap-1.5">
      {rows.map((row, index) => (
        <div key={index} className="flex flex-wrap items-center gap-1.5">
          {index === 0 ? (
            <span className="w-14 text-right text-12 text-subtle">where</span>
          ) : (
            <Select aria-label="Join with" value={row.joiner} options={JOINER_OPTIONS} onChange={(joiner) => setRow(index, { joiner })} className="w-14" />
          )}
          <input list={listId} aria-label="Field" placeholder="field" value={row.field} onChange={(event) => setRow(index, { field: event.target.value })} className={cx(CONTROL, "w-64 font-mono")} />
          <Select aria-label="Comparison" value={row.op} options={OP_OPTIONS} onChange={(op) => setRow(index, { op })} className="w-16 font-mono" />
          <input aria-label="Value" placeholder="value" value={row.value} onChange={(event) => setRow(index, { value: event.target.value })} className={cx(CONTROL, "w-28 font-mono")} />
          {rows.length > 1 && (
            <IconButton label="Remove condition" size="sm" onClick={() => onChange(rows.filter((_, at) => at !== index))}>
              <X className="size-3" />
            </IconButton>
          )}
        </div>
      ))}
      <datalist id={listId}>
        {fields.map((field) => (
          <option key={field} value={field} />
        ))}
      </datalist>
      <Button size="sm" variant="ghost" className="w-fit" onClick={() => onChange([...rows, { joiner: "and", field: "", op: ">=", value: "" }])}>
        Add condition
      </Button>
    </div>
  );
}
