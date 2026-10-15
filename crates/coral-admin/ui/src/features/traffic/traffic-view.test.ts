import { describe, expect, it } from "vitest";

import { compareFlag, DEFAULT_WINDOW, rangeQuery, trafficViewOf } from "./traffic-view";

const FROM = "2026-10-15T10:00:00.000Z";
const TO = "2026-10-15T11:00:00.000Z";

function view(query: string) {
  return trafficViewOf(new URLSearchParams(query));
}

describe("trafficViewOf", () => {
  it("opens on the default window with nothing selected", () => {
    const opened = view("");
    expect(opened.window).toBe(DEFAULT_WINDOW);
    expect(opened.chart).toEqual({ window: DEFAULT_WINDOW });
    expect(opened.scope).toEqual(opened.chart);
    expect(opened.selection).toBeNull();
    expect(opened.compare).toBe(false);
  });

  it("scopes every table to a brushed selection inside a window", () => {
    const brushed = view(`window=24h&from=${FROM}&to=${TO}`);
    expect(brushed.chart).toEqual({ window: "24h" });
    expect(brushed.selection).toEqual({ from: FROM, to: TO });
    expect(brushed.scope).toEqual({ from: FROM, to: TO });
  });

  it("charts a custom range without a selection when no window is given", () => {
    const custom = view(`from=${FROM}&to=${TO}`);
    expect(custom.window).toBeNull();
    expect(custom.chart).toEqual({ from: FROM, to: TO });
    expect(custom.scope).toEqual(custom.chart);
    expect(custom.selection).toBeNull();
  });

  it("ignores bounds that are unreadable or backwards", () => {
    expect(view(`from=${TO}&to=${FROM}`).chart).toEqual({ window: DEFAULT_WINDOW });
    expect(view(`window=1h&from=yesterday&to=${TO}`).selection).toBeNull();
  });

  it("drops unknown windows and filter values", () => {
    const unknown = view("window=2h&class=3xx&cause=bored&sort=latency&route=/v3/players&method=POST");
    expect(unknown.window).toBe(DEFAULT_WINDOW);
    expect(unknown.callers).toEqual({ route: "/v3/players", method: "POST", class: null, cause: null, sort: "requests" });
  });

  it("reads the caller filters and the comparison toggle", () => {
    const filtered = view("window=7d&class=rate_limited&cause=upstream_limit&sort=subjects&compare=1");
    expect(filtered.callers).toEqual({ route: null, method: null, class: "rate_limited", cause: "upstream_limit", sort: "subjects" });
    expect(filtered.compare).toBe(true);
  });
});

describe("compareFlag", () => {
  it("writes the toggle only when it is on", () => {
    expect(compareFlag(true)).toBe("1");
    expect(compareFlag(false)).toBeNull();
  });
});

describe("rangeQuery", () => {
  it("names a window or both bounds", () => {
    expect(rangeQuery({ window: "6h" })).toEqual({ window: "6h" });
    expect(rangeQuery({ from: FROM, to: TO })).toEqual({ from: FROM, to: TO });
  });
});
