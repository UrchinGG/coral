import { describe, expect, it } from "vitest";

import type { Coverage } from "../api/types/session";
import { coverageNotices } from "./coverage";

const COVERAGE: Coverage = {
  log_from: "2026-09-30T00:00:00.000000Z",
  detail_from: "2026-10-14T03:14:32.000000Z",
  detail_since: "2026-10-14T03:14:32.000000Z",
  attribution_from: "2026-10-14T21:00:00.000000Z",
  subjects_from: "2026-10-01T00:00:00.000000Z",
  body_subjects_from: "2026-10-14T03:14:32.000000Z",
  rollups_from: null,
  network_from: null,
  gaps: [{ feature: "subjects", from: "2026-10-14T01:00:00.000000Z", to: "2026-10-14T02:00:00.000000Z" }],
  backfill: { by_name_share: { "2026-10-03": 0.26, "20261013": 0.31 } },
  writer: { last_seen: null, lag_s: null, dropped_today: {}, unmapped_today: {} },
};

describe("coverageNotices", () => {
  it("reports a window that starts before a feature was recorded", () => {
    const notices = coverageNotices(COVERAGE, {
      features: ["body_subjects"],
      from: "2026-10-14T00:00:00.000000Z",
      to: "2026-10-14T12:00:00.000000Z",
    });
    expect(notices).toEqual([{ kind: "starts_after", feature: "body_subjects", since: "2026-10-14T03:14:32.000000Z" }]);
  });

  it("stays quiet when the window is fully covered", () => {
    const notices = coverageNotices(COVERAGE, {
      features: ["detail"],
      from: "2026-10-14T05:00:00.000000Z",
      to: "2026-10-14T06:00:00.000000Z",
    });
    expect(notices).toEqual([]);
  });

  it("reports features without any marker as not recorded", () => {
    const notices = coverageNotices(COVERAGE, {
      features: ["rollups"],
      from: "2026-10-14T05:00:00.000000Z",
      to: "2026-10-14T06:00:00.000000Z",
    });
    expect(notices).toEqual([{ kind: "not_recorded", feature: "rollups" }]);
  });

  it("reports gaps crossing the window and the backfilled days inside it", () => {
    const notices = coverageNotices(COVERAGE, {
      features: ["subjects"],
      from: "2026-10-13T12:00:00.000000Z",
      to: "2026-10-14T12:00:00.000000Z",
      clampedFrom: "2026-10-14T21:00:00.000000Z",
    });
    expect(notices).toEqual([
      { kind: "gap", feature: "subjects", from: "2026-10-14T01:00:00.000000Z", to: "2026-10-14T02:00:00.000000Z" },
      { kind: "clamped", since: "2026-10-14T21:00:00.000000Z" },
      { kind: "backfilled", day: "2026-10-13", share: 0.31 },
    ]);
  });
});
