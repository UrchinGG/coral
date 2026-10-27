const relative = new Intl.RelativeTimeFormat("en-US", { numeric: "auto" });

const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
];

export const count = (value: number) => value.toLocaleString("en-US");

export const monthYear = (iso: string) =>
  new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    year: "numeric",
  });

export function ago(iso: string) {
  const seconds = (Date.parse(iso) - Date.now()) / 1000;
  const [unit, size] =
    units.find(([, size]) => Math.abs(seconds) >= size) ?? units[2];
  return relative.format(Math.round(seconds / size), unit);
}

export function until(iso: string) {
  const date = new Date(iso);
  const sameDay = date.toDateString() === new Date().toDateString();
  return date.toLocaleString("en-US", {
    weekday: sameDay ? undefined : "short",
    hour: "numeric",
    minute: "2-digit",
  });
}
