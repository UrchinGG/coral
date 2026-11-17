const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const WEEK = 7 * DAY;
const MISSING = "—";

const date = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  year: "numeric",
});
const dateTime = new Intl.DateTimeFormat("en-US", {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});
const clock = new Intl.DateTimeFormat("en-US", {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
});

export type Timestamp = string;

export function ago(at: Timestamp | null) {
  if (at === null) return "Never";
  const elapsed = Date.now() - Date.parse(at);
  if (Number.isNaN(elapsed)) return MISSING;
  if (elapsed < MINUTE) return "Just now";
  if (elapsed < HOUR) return `${Math.floor(elapsed / MINUTE)} min ago`;
  if (elapsed < DAY) return `${Math.floor(elapsed / HOUR)} h ago`;
  if (elapsed < WEEK) return `${Math.floor(elapsed / DAY)} d ago`;
  return day(at);
}

const safely = (format: Intl.DateTimeFormat) => (at: Timestamp) => {
  const when = new Date(at);
  return Number.isNaN(when.getTime()) ? MISSING : format.format(when);
};

export const day = safely(date);

export const moment = safely(dateTime);

export const time = safely(clock);
