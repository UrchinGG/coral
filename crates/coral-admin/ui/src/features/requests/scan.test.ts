import { describe, expect, it } from "vitest";

import type { Page } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";
import { cursorOf, scannedHours, scanProgress, shouldAutoContinue, timestampMicros } from "./scan";

const LABELS = { discord: {}, minecraft: {}, pending: { discord: [], minecraft: [] } };
const TOP = Date.parse("2026-10-14T12:00:00Z");

function page(next: string | null, scannedTo: string | null): Page<RequestRow> {
  return { items: [], next_cursor: next, scanned_to: scannedTo, clamped_from: null, labels: LABELS };
}

describe("scanProgress", () => {
  it("is not partial while pages come back full or the scan has ended", () => {
    expect(scanProgress([page("c1", null)], TOP).partial).toBe(false);
    expect(scanProgress([page(null, null)], TOP).partial).toBe(false);
    expect(scanProgress([], TOP).partial).toBe(false);
  });

  it("measures how far a time-boxed scan has reached below the top of the window", () => {
    const progress = scanProgress([page("c1", null), page("c2", "2026-10-14T09:30:00.000000Z")], TOP);
    expect(progress).toEqual({ partial: true, scannedTo: "2026-10-14T09:30:00.000000Z", scannedMs: 2.5 * 3_600_000 });
  });

  it("continues on its own only inside the scanned-time budget", () => {
    const shallow = scanProgress([page("c", "2026-10-14T00:00:00.000000Z")], TOP);
    const deep = scanProgress([page("c", "2026-10-13T11:00:00.000000Z")], TOP);
    expect(shouldAutoContinue(shallow, 24)).toBe(true);
    expect(shouldAutoContinue(deep, 24)).toBe(false);
    expect(shouldAutoContinue(deep, 48)).toBe(true);
  });

  it("continues from where a scan that overshot its budget stopped", () => {
    const overshot = scanProgress([page("c", "2026-10-12T11:00:00.000000Z")], TOP);
    expect(scannedHours(overshot)).toBe(49);
    expect(shouldAutoContinue(overshot, 24 + 24)).toBe(false);
    expect(shouldAutoContinue(overshot, scannedHours(overshot) + 24)).toBe(true);
  });
});

describe("cursorOf", () => {
  it("encodes the timestamp in microseconds and the request id like the server", () => {
    const cursor = cursorOf({ ts: "2026-10-14T21:41:07.556439Z", request_id: "0199e4a17c2b7d108a3e2f1d9c4b5a60" });
    expect(cursor).toBe("AAZd08nt4FcBmeShfCt9EIo-Lx2cS1pg");
  });

  it("reads timestamps with any number of fraction digits", () => {
    expect(timestampMicros("2026-10-14T21:41:07Z")).toBe(1_792_014_067_000_000n);
    expect(timestampMicros("2026-10-14T21:41:07.5Z")).toBe(1_792_014_067_500_000n);
    expect(() => timestampMicros("yesterday")).toThrow(RangeError);
  });
});
