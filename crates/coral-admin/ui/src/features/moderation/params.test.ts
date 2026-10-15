import { describe, expect, it } from "vitest";

import { blacklistFiltersOf, strikeFilterOf, tabOf } from "./params";

describe("moderation URL state", () => {
  it("reads blacklist filters and falls back on unknown values", () => {
    expect(blacklistFiltersOf(new URLSearchParams("q=Kestrel_&field=tagger&tag=sniper&dir=asc"))).toEqual({ q: "Kestrel_", field: "tagger", tag: "sniper", dir: "asc" });
    expect(blacklistFiltersOf(new URLSearchParams("field=everything&dir=sideways"))).toEqual({ q: "", field: "player", tag: "", dir: "desc" });
  });

  it("defaults to the blacklist tab and active strikes", () => {
    expect(tabOf(new URLSearchParams("tab=guide"))).toBe("guide");
    expect(tabOf(new URLSearchParams("tab=nope"))).toBe("blacklist");
    expect(strikeFilterOf(new URLSearchParams())).toBe("active");
  });
});
