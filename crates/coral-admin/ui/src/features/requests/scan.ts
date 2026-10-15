import type { Page } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";

export type ScanProgress = {
  partial: boolean;
  scannedTo: string | null;
  scannedMs: number;
};

const HOUR_MS = 3_600_000;
const MICROS_PER_SECOND = 1_000_000n;
const MS_PER_SECOND = 1_000;
const FRACTION_DIGITS = 6;
const MICROS_BYTES = 8;
const ID_BYTES = 16;
const BYTE_BITS = 8n;
const BYTE_MASK = 0xffn;
const HEX_RADIX = 16;
const FRACTIONAL_TIMESTAMP = /^(.*?)(?:\.(\d+))?Z$/;

export function scanProgress(pages: readonly Page<RequestRow>[], topMs: number): ScanProgress {
  const last = pages.at(-1);
  if (last === undefined || last.scanned_to === null || last.next_cursor === null) return { partial: false, scannedTo: null, scannedMs: 0 };
  return { partial: true, scannedTo: last.scanned_to, scannedMs: Math.max(0, topMs - Date.parse(last.scanned_to)) };
}

export function shouldAutoContinue(progress: ScanProgress, budgetHours: number): boolean {
  return progress.partial && progress.scannedMs < budgetHours * HOUR_MS;
}

export function scannedHours(progress: ScanProgress): number {
  return progress.scannedMs / HOUR_MS;
}

export function cursorOf(row: Pick<RequestRow, "ts" | "request_id">): string {
  const bytes = new Uint8Array(MICROS_BYTES + ID_BYTES);
  let micros = timestampMicros(row.ts);
  for (let index = MICROS_BYTES - 1; index >= 0; index -= 1) {
    bytes[index] = Number(micros & BYTE_MASK);
    micros >>= BYTE_BITS;
  }
  for (let index = 0; index < ID_BYTES; index += 1) bytes[MICROS_BYTES + index] = Number.parseInt(row.request_id.slice(index * 2, index * 2 + 2), HEX_RADIX);
  return base64Url(bytes);
}

export function timestampMicros(iso: string): bigint {
  const match = FRACTIONAL_TIMESTAMP.exec(iso);
  if (match === null) throw new RangeError(`${iso} is not a UTC timestamp`);
  const seconds = BigInt(Math.floor(Date.parse(`${match[1]}Z`) / MS_PER_SECOND));
  const fraction = BigInt((match[2] ?? "").padEnd(FRACTION_DIGITS, "0").slice(0, FRACTION_DIGITS));
  return seconds * MICROS_PER_SECOND + fraction;
}

function base64Url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
