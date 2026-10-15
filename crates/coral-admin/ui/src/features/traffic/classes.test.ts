import { describe, expect, it } from "vitest";

import type { SeriesPoint, TrafficSeries } from "../../api/types/traffic";
import { CLASS_KEYS, CLASS_SERIES, changeOf, rolledPoints, shareOf } from "./classes";

describe("shareOf", () => {
  it("is the part of a non-empty total", () => {
    expect(shareOf(5, 20)).toBe(0.25);
  });

  it("is unknown for an empty total", () => {
    expect(shareOf(0, 0)).toBeNull();
  });
});

describe("changeOf", () => {
  it("is the relative change from the earlier value", () => {
    expect(changeOf(150, 100)).toBe(0.5);
    expect(changeOf(50, 100)).toBe(-0.5);
  });

  it("is unknown without an earlier value to compare with", () => {
    expect(changeOf(10, 0)).toBeNull();
  });
});

describe("CLASS_SERIES", () => {
  it("stacks every status class once, in class order", () => {
    expect(CLASS_SERIES.map((series) => series.key)).toEqual([...CLASS_KEYS]);
  });
});

describe("rolledPoints", () => {
  const point = (t: string): SeriesPoint => ({ t, requests: 1, c2xx: 1, c4xx: 0, c429_caller: 0, c429_upstream: 0, c5xx: 0, p95: null });
  const series = (rolledTo: string | null): TrafficSeries => ({
    from: "2026-10-15T12:00:00.000000Z",
    to: "2026-10-15T12:03:00.000000Z",
    step_s: 60,
    points: [point("2026-10-15T12:00:00.000000Z"), point("2026-10-15T12:01:00.000000Z"), point("2026-10-15T12:02:00.000000Z")],
    last_partial: true,
    rolled_to: rolledTo,
  });

  it("keeps only buckets the rollups have finished", () => {
    expect(rolledPoints(series("2026-10-15T12:02:00.000000Z")).map((kept) => kept.t)).toEqual(["2026-10-15T12:00:00.000000Z", "2026-10-15T12:01:00.000000Z"]);
  });

  it("keeps nothing before the rollups have run", () => {
    expect(rolledPoints(series(null))).toEqual([]);
  });
});
