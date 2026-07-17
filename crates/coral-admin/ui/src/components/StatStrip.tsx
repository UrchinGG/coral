export type Stat = {
  label: string;
  value: string;
  sub?: string;
  tone?: "default" | "danger" | "warning" | "ok";
};

const TONE_TEXT: Record<NonNullable<Stat["tone"]>, string> = {
  default: "text-gray-100",
  danger: "text-danger",
  warning: "text-warning",
  ok: "text-ok",
};

export function StatStrip({ stats }: { stats: Stat[] }) {
  return (
    <div className="grid grid-cols-2 divide-x divide-y divide-white/8 overflow-hidden rounded-xl border border-white/8 bg-white/[0.03] sm:grid-cols-4 sm:divide-y-0">
      {stats.map((s) => (
        <div key={s.label} className="p-4">
          <div className="text-[11px] font-medium tracking-wide text-gray-500 uppercase">{s.label}</div>
          <div className={`mt-1 text-xl font-semibold ${TONE_TEXT[s.tone ?? "default"]}`}>{s.value}</div>
          {s.sub && <div className="mt-0.5 text-xs text-gray-500">{s.sub}</div>}
        </div>
      ))}
    </div>
  );
}
