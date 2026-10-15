import type { ReactNode } from "react";

type PageHeadProps = { title: ReactNode; lede?: ReactNode; actions?: ReactNode };

export function PageHead({ title, lede, actions }: PageHeadProps) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div className="flex min-w-0 flex-col gap-1">
        <h1 className="text-22 font-semibold wrap-anywhere">{title}</h1>
        {lede !== undefined && <p className="m-0 max-w-[80ch] text-muted">{lede}</p>}
      </div>
      {actions !== undefined && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}
