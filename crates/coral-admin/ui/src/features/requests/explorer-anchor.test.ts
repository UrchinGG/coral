import { describe, expect, it } from "vitest";

import { loaderAnchorFor, rememberLoaderAnchor } from "./explorer-anchor";
import { EMPTY_FILTER } from "./explorer-filter";

describe("the loader's window anchor", () => {
  it("goes only to the page that renders the filter the loader prefetched", () => {
    const anchoredAt = Date.parse("2026-10-14T12:00:00.000Z");
    rememberLoaderAnchor(EMPTY_FILTER, anchoredAt);
    expect(loaderAnchorFor({ ...EMPTY_FILTER })).toBe(anchoredAt);
    expect(loaderAnchorFor({ ...EMPTY_FILTER, method: "GET" })).toBeNull();
  });
});
