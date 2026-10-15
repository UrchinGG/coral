export type BuilderRow = { joiner: Joiner; field: string; op: CompareOp; value: string };

export type Joiner = "and" | "or";
export type CompareOp = (typeof COMPARE_OPS)[number];

export const COMPARE_OPS = [">=", ">", "==", "!=", "<", "<="] as const;

const FIRST_FIELD = "achievements.bedwars_level";

export const EMPTY_BUILDER: BuilderRow[] = [{ joiner: "and", field: FIRST_FIELD, op: ">=", value: "" }];

export function conditionOf(rows: readonly BuilderRow[]): string {
  if (rows.some((row) => row.field.trim() === "" || row.value.trim() === "")) return "";
  return rows.map((row, index) => clauseOf(row, index === 0)).join(" ");
}

function clauseOf(row: BuilderRow, first: boolean): string {
  const raw = row.value.trim();
  const value = Number.isNaN(Number(raw)) ? `"${raw}"` : raw;
  const clause = `${row.field.trim()} ${row.op} ${value}`;
  return first ? clause : `${row.joiner} ${clause}`;
}
