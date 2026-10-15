import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { cx } from "../lib/cx";
import { RefetchBar } from "./States";

type SectionProps = {
  title: ReactNode;
  icon?: LucideIcon;
  count?: ReactNode;
  actions?: ReactNode;
  flush?: boolean;
  fetching?: boolean;
  id?: string;
  className?: string;
  children: ReactNode;
};

export function Section({ title, icon: Icon, count, actions, flush = false, fetching = false, id, className, children }: SectionProps) {
  return (
    <section id={id} className={cx("relative min-w-0 rounded-md border border-border bg-surface", className)}>
      {fetching && <RefetchBar />}
      <div className="flex min-h-[42px] flex-wrap items-center justify-between gap-2.5 border-b border-border px-3.5 py-2.5">
        <h2 className="flex items-center gap-2 text-13 font-semibold">
          {Icon !== undefined && <Icon className="size-3.5 shrink-0" />}
          {title}
          {count !== undefined && <span className="font-medium text-subtle">{count}</span>}
        </h2>
        {actions !== undefined && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
      <div className={flush ? undefined : "px-3.5 py-3"}>{children}</div>
    </section>
  );
}
