export type PingRecord = {
  day: string;
  timestamp: number;
  max: number;
  min: number;
  avg: number;
};

export function getLatestPing(
  records: PingRecord[] = []
): PingRecord | undefined {
  if (!records || records.length === 0) return undefined;
  return [...records].sort((a, b) => b.timestamp - a.timestamp)[0];
}

export function getPingIcon(avgMs?: number): string {
  if (avgMs === undefined || avgMs === null || Number.isNaN(avgMs))
    return "/ping/ping_1.png";
  if (avgMs <= 50) return "/ping/ping_5.png";
  if (avgMs <= 100) return "/ping/ping_4.png";
  if (avgMs <= 180) return "/ping/ping_3.png";
  if (avgMs <= 220) return "/ping/ping_2.png";
  return "/ping/ping_1.png";
}
