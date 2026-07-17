export function fmtNum(v: number): string {
  return Math.round(v).toLocaleString();
}

export function fmtMs(v: number | null | undefined): string {
  if (v === null || v === undefined) return "—";
  if (v <= 0) return "0";
  if (v < 1) return "<1 ms";
  if (v < 1000) return `${Math.round(v)} ms`;
  return `${(v / 1000).toFixed(2)} s`;
}

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString();
}

export function fmtPercent(v: number): string {
  return `${(v * 100).toFixed(1)}%`;
}

export const STATUS_COLORS = {
  s2xx: "bg-ok",
  s3xx: "bg-gray-400",
  s4xx: "bg-warning",
  s5xx: "bg-danger",
};

export function accessRankLabel(level: number): string {
  if (level >= 5) return "Owner";
  if (level === 4) return "Admin";
  if (level === 3) return "Moderator";
  if (level === 2) return "Helper";
  return "Default";
}

export function accessRankTone(level: number): "default" | "accent" {
  return level >= 2 ? "accent" : "default";
}

export function prettyJson(value: unknown): string {
  if (typeof value !== "string") return JSON.stringify(value, null, 2);
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
