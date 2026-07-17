import type { ReactNode } from "react";

type PanelProps = {
  title?: string;
  description?: string;
  action?: ReactNode;
  bare?: boolean;
  className?: string;
  children: ReactNode;
};

export function Panel({ title, description, action, bare = false, className = "", children }: PanelProps) {
  const frame = bare ? "" : "rounded-xl border border-white/8 bg-white/[0.03] p-5";
  return (
    <section className={`${frame} ${className}`}>
      {(title || action) && (
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            {title && <h2 className="text-sm font-medium text-gray-200">{title}</h2>}
            {description && <p className="mt-0.5 text-xs text-gray-500">{description}</p>}
          </div>
          {action}
        </div>
      )}
      {children}
    </section>
  );
}
