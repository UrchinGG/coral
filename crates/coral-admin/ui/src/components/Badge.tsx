type BadgeProps = {
  label: string;
  tone?: "default" | "danger" | "warning" | "ok" | "accent";
};

const TONE_CLASSES: Record<NonNullable<BadgeProps["tone"]>, string> = {
  default: "bg-white/8 text-gray-400",
  danger: "bg-danger/15 text-danger",
  warning: "bg-warning/15 text-warning",
  ok: "bg-ok/15 text-ok",
  accent: "bg-accent/15 text-accent",
};

export function Badge({ label, tone = "default" }: BadgeProps) {
  return <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${TONE_CLASSES[tone]}`}>{label}</span>;
}
