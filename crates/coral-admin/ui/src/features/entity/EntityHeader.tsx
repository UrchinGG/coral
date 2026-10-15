import type { ReactNode } from "react";

import type { EntityKind } from "../../api/types/refs";
import { ENTITY_SOFT } from "../../kit/entity-style";
import { cx } from "../../lib/cx";

type EntityHeaderProps = {
  icon: ReactNode;
  kind: EntityKind;
  title: ReactNode;
  badges?: ReactNode;
  ids?: ReactNode;
  meta?: readonly ReactNode[];
  actions?: ReactNode;
  round?: boolean;
};

export function EntityHeader({ icon, kind, title, badges, ids, meta = [], actions, round = false }: EntityHeaderProps) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-4 rounded-md border border-border bg-surface p-4">
      <div className="flex min-w-0 flex-[1_1_420px] items-start gap-3.5">
        <div aria-hidden className={cx("flex size-11 shrink-0 items-center justify-center text-18 font-bold", round ? "rounded-full" : "rounded-lg", ENTITY_SOFT[kind])}>
          {icon}
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="text-22 font-semibold break-all max-md:text-18">{title}</h1>
            {badges}
          </div>
          {ids}
          {meta.length > 0 && (
            <div className="flex flex-wrap gap-x-3.5 gap-y-1 text-12 text-muted">
              {meta.map((item, index) => (
                <span key={index}>{item}</span>
              ))}
            </div>
          )}
        </div>
      </div>
      {actions !== undefined && <div className="flex flex-wrap items-start gap-1.5">{actions}</div>}
    </header>
  );
}

export function IdRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 items-start gap-2">
      <span className="inline-block min-w-[78px] pt-0.5 text-11 tracking-[0.05em] text-subtle uppercase">{label}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}
