const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

const date = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
});

export const toDate = (micros: number) => new Date(micros / 1000);

export function ago(micros: number | null) {
  if (micros === null) return "Never";
  const elapsed = Date.now() - micros / 1000;
  if (elapsed < MINUTE) return "Just now";
  if (elapsed < HOUR) return `${Math.floor(elapsed / MINUTE)} min ago`;
  if (elapsed < DAY) return `${Math.floor(elapsed / HOUR)} h ago`;
  if (elapsed < 7 * DAY) return `${Math.floor(elapsed / DAY)} d ago`;
  return date.format(toDate(micros));
}

export const day = (micros: number) => date.format(toDate(micros));
