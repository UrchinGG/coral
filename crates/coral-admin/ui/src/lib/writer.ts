import type { WriterHealth } from "../api/types/session";
import { formatNumber } from "./format";

export type WriterReading = { healthy: boolean; text: string };

const WRITER_LAG_LIMIT_S = 120;

export function readWriter(writer: WriterHealth): WriterReading {
  if (writer.last_seen === null || writer.lag_s === null) return { healthy: false, text: "writer not seen yet" };
  const dropped = sum(writer.dropped_today);
  const unmapped = Object.keys(writer.unmapped_today).length;
  const problems = [
    writer.lag_s > WRITER_LAG_LIMIT_S ? `lag ${formatNumber(writer.lag_s)} s` : null,
    dropped > 0 ? `${formatNumber(dropped)} dropped today` : null,
    unmapped > 0 ? `${formatNumber(unmapped)} unmapped routes today` : null,
  ].filter((problem) => problem !== null);
  if (problems.length > 0) return { healthy: false, text: `writer: ${problems.join(" · ")}` };
  return { healthy: true, text: `writer healthy · lag ${formatNumber(writer.lag_s)} s` };
}

function sum(counts: Record<string, number>): number {
  return Object.values(counts).reduce((total, count) => total + count, 0);
}
