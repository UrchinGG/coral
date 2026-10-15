import { SCAN_AUTO_CONTINUE_HOURS } from "../../api/queryClient";
import type { Page } from "../../api/types/refs";
import { parseTimestamp } from "../../lib/time";

export type ScanState = { scannedTo: string | null; clampedFrom: string | null };

export type ScanInput = { scannedTo: string | null; hasNextPage: boolean; loadedRows: number; budgetStart: number };

export type ScanProgress = { active: boolean; exhausted: boolean };

const HOUR_MS = 3_600_000;
const SCAN_BUDGET_MS = SCAN_AUTO_CONTINUE_HOURS * HOUR_MS;

export const FILLED_ROWS = 40;

export function scanStateOf<T>(pages: readonly Page<T>[]): ScanState {
  const last = pages.at(-1);
  return { scannedTo: last?.scanned_to ?? null, clampedFrom: pages.find((page) => page.clamped_from !== null)?.clamped_from ?? null };
}

export function scanProgress({ scannedTo, hasNextPage, loadedRows, budgetStart }: ScanInput): ScanProgress {
  if (scannedTo === null || !hasNextPage || loadedRows >= FILLED_ROWS) return { active: false, exhausted: false };
  const scannedMs = budgetStart - parseTimestamp(scannedTo).getTime();
  const active = scannedMs < SCAN_BUDGET_MS;
  return { active, exhausted: !active };
}
