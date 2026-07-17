type HealthLevel = "ok" | "warning" | "danger";

const DOT: Record<HealthLevel, string> = {
  ok: "bg-ok",
  warning: "bg-warning",
  danger: "bg-danger",
};

const TEXT: Record<HealthLevel, string> = {
  ok: "text-gray-200",
  warning: "text-warning",
  danger: "text-danger",
};

export function HealthSignal({ level, message }: { level: HealthLevel; message: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className={`h-2 w-2 rounded-full ${DOT[level]}`} />
      <span className={`text-sm font-medium ${TEXT[level]}`}>{message}</span>
    </div>
  );
}
