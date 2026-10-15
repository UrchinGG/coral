export type QueryTiming = {
  endpoint: string;
  status: number;
  serverMs: number | null;
  clientMs: number;
  at: number;
};

const APP_METRIC = "app";
const DURATION_PARAM = /(?:^|;)\s*dur=([0-9.]+)/;

const timings = new Map<string, QueryTiming>();
const listeners = new Set<() => void>();

export function parseServerTiming(header: string | null): number | null {
  if (header === null) return null;
  const metric = header
    .split(",")
    .map((entry) => entry.trim())
    .find((entry) => entry.split(";")[0].trim() === APP_METRIC);
  const duration = metric === undefined ? null : DURATION_PARAM.exec(metric);
  return duration === null ? null : Number.parseFloat(duration[1]);
}

export function recordTiming(queryHash: string, timing: QueryTiming): void {
  timings.set(queryHash, timing);
  notify();
}

export function forgetTiming(queryHash: string): void {
  if (timings.delete(queryHash)) notify();
}

export function timingOf(queryHash: string): QueryTiming | null {
  return timings.get(queryHash) ?? null;
}

export function subscribeTimings(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notify(): void {
  listeners.forEach((listener) => listener());
}
