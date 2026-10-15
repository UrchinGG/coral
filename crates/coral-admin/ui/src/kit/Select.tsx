import { useId, type SelectHTMLAttributes } from "react";

import { cx } from "../lib/cx";
import { CONTROL } from "./control-styles";
import { Field } from "./Input";

export type SelectOption<V extends string> = { value: V; label: string };

type SelectProps<V extends string> = Omit<SelectHTMLAttributes<HTMLSelectElement>, "value" | "onChange"> & {
  label?: string;
  value: V;
  options: readonly SelectOption<V>[];
  onChange: (value: V) => void;
};

export function Select<V extends string>({ label, value, options, onChange, className, id, ...rest }: SelectProps<V>) {
  const generatedId = useId();
  const selectId = id ?? generatedId;
  const select = (
    <select
      id={selectId}
      value={value}
      onChange={(event) => {
        const option = options.find((candidate) => candidate.value === event.target.value);
        if (option !== undefined) onChange(option.value);
      }}
      className={cx(CONTROL, "pr-6", className)}
      {...rest}
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
  if (label === undefined) return select;
  return (
    <Field label={label} htmlFor={selectId}>
      {select}
    </Field>
  );
}
