import type { ViewField } from "../../api/types/extension";

export type FieldValues = Record<string, string>;

export function initialValues(fields: readonly ViewField[]): FieldValues {
  return Object.fromEntries(fields.map((field) => [field.name, initialValue(field)]));
}

export function missingRequired(fields: readonly ViewField[], values: FieldValues): boolean {
  return fields.some((field) => field.required && (values[field.name] ?? "").trim() === "");
}

function initialValue(field: ViewField): string {
  switch (field.kind) {
    case "select":
      return field.value ?? field.options[0]?.value ?? "";
    case "secret":
      return "";
    case "text":
      return field.value ?? "";
  }
}
