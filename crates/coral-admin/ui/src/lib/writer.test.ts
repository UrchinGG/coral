import { describe, expect, it } from "vitest";

import type { WriterHealth } from "../api/types/session";
import { readWriter } from "./writer";

const HEALTHY: WriterHealth = { last_seen: "2026-10-15T12:00:00.000000Z", lag_s: 2, dropped_today: {}, unmapped_today: {} };

describe("readWriter", () => {
  it("reads a writer that has never reported as unhealthy", () => {
    expect(readWriter({ ...HEALTHY, last_seen: null, lag_s: null })).toEqual({ healthy: false, text: "writer not seen yet" });
  });

  it("states the lag of a healthy writer", () => {
    expect(readWriter(HEALTHY)).toEqual({ healthy: true, text: "writer healthy · lag 2 s" });
  });

  it("lists every problem of an unhealthy writer", () => {
    const failing: WriterHealth = { ...HEALTHY, lag_s: 1_250, dropped_today: { channel_full: 1_200, timeout: 4 }, unmapped_today: { "GET /x": 3, "GET /y": 1 } };
    expect(readWriter(failing)).toEqual({ healthy: false, text: "writer: lag 1,250 s · 1,204 dropped today · 2 unmapped routes today" });
  });
});
