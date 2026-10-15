import { describe, expect, it } from "vitest";

import type { Page } from "../../api/types/refs";
import { FILLED_ROWS, scanProgress, scanStateOf } from "./scan";

const NOW = Date.parse("2026-10-15T12:00:00Z");
const HOUR_MS = 3_600_000;

function page(scannedTo: string | null, clampedFrom: string | null = null): Page<number> {
  return { items: [], next_cursor: "next", scanned_to: scannedTo, clamped_from: clampedFrom, labels: { discord: {}, minecraft: {}, pending: { discord: [], minecraft: [] } } };
}

function hoursBefore(hours: number): string {
  return new Date(NOW - hours * HOUR_MS).toISOString();
}

describe("scanStateOf", () => {
  it("reads the resume point from the last page and the clamp from any page", () => {
    const state = scanStateOf([page(hoursBefore(1), "2026-10-14T21:00:00Z"), page(hoursBefore(3))]);
    expect(state).toEqual({ scannedTo: hoursBefore(3), clampedFrom: "2026-10-14T21:00:00Z" });
  });
});

describe("scanProgress", () => {
  it("keeps scanning a thin time-boxed stream inside its budget", () => {
    expect(scanProgress({ scannedTo: hoursBefore(5), hasNextPage: true, loadedRows: 3, budgetStart: NOW })).toEqual({ active: true, exhausted: false });
  });

  it("stops after the budget and offers to continue", () => {
    expect(scanProgress({ scannedTo: hoursBefore(25), hasNextPage: true, loadedRows: 3, budgetStart: NOW })).toEqual({ active: false, exhausted: true });
  });

  it("leaves a filled or direct stream to scrolling", () => {
    expect(scanProgress({ scannedTo: hoursBefore(5), hasNextPage: true, loadedRows: FILLED_ROWS, budgetStart: NOW })).toEqual({ active: false, exhausted: false });
    expect(scanProgress({ scannedTo: null, hasNextPage: true, loadedRows: 0, budgetStart: NOW })).toEqual({ active: false, exhausted: false });
  });

  it("measures a continued scan from where the previous budget ended", () => {
    const resumedAt = Date.parse(hoursBefore(25));
    expect(scanProgress({ scannedTo: hoursBefore(30), hasNextPage: true, loadedRows: 3, budgetStart: resumedAt })).toEqual({ active: true, exhausted: false });
  });
});
