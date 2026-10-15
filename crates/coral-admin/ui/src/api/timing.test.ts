import { describe, expect, it } from "vitest";

import { parseServerTiming } from "./timing";

describe("parseServerTiming", () => {
  it("reads the app metric duration", () => {
    expect(parseServerTiming("app;dur=12.5")).toBe(12.5);
  });

  it("finds the app metric among others", () => {
    expect(parseServerTiming('db;dur=3, app;desc="handler";dur=41.0, cache;desc=hit')).toBe(41);
  });

  it("returns null when the header or the metric is absent", () => {
    expect(parseServerTiming(null)).toBeNull();
    expect(parseServerTiming("db;dur=3")).toBeNull();
    expect(parseServerTiming("app;desc=nothing")).toBeNull();
  });
});
