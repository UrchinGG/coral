import type { ViewField } from "../../api/types/extension";
import type { FieldValues } from "./field-values";
import { Input } from "../../kit/Input";
import { Select } from "../../kit/Select";

type ViewFieldsProps = { fields: readonly ViewField[]; values: FieldValues; onChange: (values: FieldValues) => void };

export function ViewFields({ fields, values, onChange }: ViewFieldsProps) {
  const set = (name: string, value: string) => onChange({ ...values, [name]: value });
  return (
    <div className="flex flex-col gap-3">
      {fields.map((field) => (
        <ViewFieldInput key={field.name} field={field} value={values[field.name] ?? ""} onChange={(value) => set(field.name, value)} />
      ))}
    </div>
  );
}

function ViewFieldInput({ field, value, onChange }: { field: ViewField; value: string; onChange: (value: string) => void }) {
  switch (field.kind) {
    case "select":
      return <Select label={field.label} value={value} options={field.options} onChange={onChange} />;
    case "secret":
      return <Input label={field.label} type="password" mono required={field.required} placeholder={field.placeholder ?? undefined} value={value} onChange={(event) => onChange(event.target.value)} />;
    case "text":
      return <Input label={field.label} required={field.required} placeholder={field.placeholder ?? undefined} value={value} onChange={(event) => onChange(event.target.value)} />;
  }
}
