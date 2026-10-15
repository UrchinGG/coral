import type { ReactNode } from "react";

import { cx } from "../lib/cx";

export type SegmentedOption<V extends string> = { value: V; label: ReactNode; title?: string };

type SegmentedProps<V extends string> = {
  label: string;
  value: V | null;
  options: readonly SegmentedOption<V>[];
  onChange: (value: V) => void;
  className?: string;
};

export function Segmented<V extends string>({ label, value, options, onChange, className }: SegmentedProps<V>) {
  return (
    <div role="group" aria-label={label} className={cx("inline-flex flex-wrap gap-0.5 rounded-md border border-border bg-bg p-0.5", className)}>
      {options.map((option) => {
        const pressed = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            title={option.title}
            aria-pressed={pressed}
            onClick={() => onChange(option.value)}
            className={cx(
              "h-[22px] rounded px-[9px] text-12 whitespace-nowrap transition-colors",
              pressed ? "bg-overlay text-fg shadow-[inset_0_0_0_1px_var(--color-border-strong)]" : "text-muted hover:text-fg",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
