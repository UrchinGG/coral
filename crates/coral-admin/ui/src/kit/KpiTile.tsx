import type { ReactNode } from "react";

import { cx } from "../lib/cx";

type KpiStripProps = { compact?: boolean; children: ReactNode };
type KpiTileProps = {
  label: ReactNode;
  value: ReactNode;
  unit?: ReactNode;
  sub?: ReactNode;
  swatch?: string;
  children?: ReactNode;
};

export function KpiStrip({ compact = false, children }: KpiStripProps) {
  return (
    <div
      data-compact={compact || undefined}
      className="group/kpis flex flex-wrap gap-px overflow-hidden rounded-md border border-border bg-border"
    >
      {children}
    </div>
  );
}

export function KpiTile({ label, value, unit, sub, swatch, children }: KpiTileProps) {
  return (
    <div className={cx("flex min-w-0 flex-[1_1_150px] flex-col gap-1 bg-surface px-3.5 pt-[11px] pb-3 group-data-[compact]/kpis:basis-[120px]")}>
      <div className="flex items-center gap-1.5 text-11 font-semibold tracking-[0.06em] text-subtle uppercase">
        {swatch !== undefined && <span className="size-2 shrink-0 rounded-sm" style={{ backgroundColor: swatch }} />}
        {label}
      </div>
      <div className="text-22 leading-[1.15] font-semibold tracking-[-0.01em] whitespace-nowrap max-md:text-18">
        {value}
        {unit !== undefined && <small className="ml-1 text-13 font-medium text-muted">{unit}</small>}
      </div>
      {sub !== undefined && <div className="flex flex-wrap items-center gap-1.5 text-11 text-muted">{sub}</div>}
      {children}
    </div>
  );
}
