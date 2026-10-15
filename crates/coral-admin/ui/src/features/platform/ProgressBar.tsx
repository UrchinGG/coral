import { formatNumber, formatPercent } from "../../lib/format";

type ProgressBarProps = { done: number; total: number; unit: string };

export function ProgressBar({ done, total, unit }: ProgressBarProps) {
  const ratio = total > 0 ? Math.min(1, done / total) : 0;
  return (
    <div className="flex min-w-[180px] items-center gap-2 text-12">
      <div role="progressbar" aria-valuemin={0} aria-valuemax={total} aria-valuenow={done} aria-label={unit} className="h-1.5 min-w-15 flex-1 overflow-hidden rounded-[3px] bg-surface-2 shadow-[inset_0_0_0_1px_var(--color-border)]">
        <div className="h-full rounded-[3px] bg-accent transition-[width] duration-300" style={{ width: formatPercent(ratio) }} />
      </div>
      <span className="font-mono whitespace-nowrap">
        {formatNumber(done)} / {formatNumber(total)} <span className="text-subtle">{unit}</span>
      </span>
    </div>
  );
}
