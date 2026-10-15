import type { ReactNode } from "react";

import { cx } from "../lib/cx";
import { formatNumber } from "../lib/format";
import { TONE_FILL, type Tone } from "./tone";

type MeterProps = {
  used: number;
  limit: number;
  label?: ReactNode;
  unit?: string;
  tone?: Tone;
  inline?: boolean;
};

const WARNING_RATIO = 0.8;
const DANGER_RATIO = 0.95;

export function Meter({ used, limit, label, unit, tone = "accent", inline = false }: MeterProps) {
  const ratio = limit > 0 ? Math.min(1, used / limit) : 0;
  const fill: Tone = ratio >= DANGER_RATIO ? "danger" : ratio >= WARNING_RATIO ? "warning" : tone;
  const reading = (
    <span className="font-mono whitespace-nowrap">
      {formatNumber(used)} / {formatNumber(limit)}
      {unit !== undefined && <span className="ml-1 text-subtle">{unit}</span>}
    </span>
  );
  const track = (
    <div
      role="meter"
      aria-valuemin={0}
      aria-valuemax={limit}
      aria-valuenow={used}
      aria-label={typeof label === "string" ? label : undefined}
      className={cx("h-1.5 overflow-hidden rounded-[3px] bg-surface-2 shadow-[inset_0_0_0_1px_var(--color-border)]", inline && "min-w-15 flex-1")}
    >
      <div className={cx("h-full rounded-[3px] transition-[width] duration-300", TONE_FILL[fill])} style={{ width: `${(ratio * 100).toFixed(1)}%` }} />
    </div>
  );
  if (inline) {
    return (
      <div className="flex min-w-[140px] items-center gap-2 text-12">
        {track}
        {reading}
      </div>
    );
  }
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <div className="flex justify-between gap-2 text-12">
        <span>{label}</span>
        {reading}
      </div>
      {track}
    </div>
  );
}
