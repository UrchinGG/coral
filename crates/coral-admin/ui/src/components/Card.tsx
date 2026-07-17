type CardProps = {
  label: string;
  value: string;
  sub?: string;
  tone?: "default" | "danger" | "warning";
};

const TONE_CLASSES: Record<NonNullable<CardProps["tone"]>, string> = {
  default: "text-gray-100",
  danger: "text-danger",
  warning: "text-warning",
};

export function Card({ label, value, sub, tone = "default" }: CardProps) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/5 p-4">
      <div className="text-xs uppercase tracking-wide text-gray-400">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${TONE_CLASSES[tone]}`}>{value}</div>
      {sub && <div className="mt-1 text-xs text-gray-500">{sub}</div>}
    </div>
  );
}
