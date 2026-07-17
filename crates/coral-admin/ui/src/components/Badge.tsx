type BadgeProps = {
  label: string;
  tone?: "default" | "danger" | "warning" | "ok";
};

const TONE_CLASSES: Record<NonNullable<BadgeProps["tone"]>, string> = {
  default: "bg-white/10 text-gray-300",
  danger: "bg-danger/15 text-danger",
  warning: "bg-warning/15 text-warning",
  ok: "bg-ok/15 text-ok",
};

export function Badge({ label, tone = "default" }: BadgeProps) {
  return <span className={`rounded-full px-2 py-0.5 text-xs ${TONE_CLASSES[tone]}`}>{label}</span>;
}
