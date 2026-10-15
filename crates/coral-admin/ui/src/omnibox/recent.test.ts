import { describe, expect, it } from "vitest";

import { parseRecent, RECENT_LIMIT, toStoredRef } from "./recent";

describe("parseRecent", () => {
  it("keeps valid refs and drops malformed entries", () => {
    const parsed = parseRecent([
      { kind: "player", uuid: "94589c87999b4a7ab3add2ae50711e64" },
      { kind: "account", discord_id: "1218840276405911602", member_id: 23025 },
      { kind: "key", key_id: null, prefix: "e91c0d3a", key_kind: "invalid" },
      { kind: "key", key_id: 4, prefix: "e91c0d3a", key_kind: "stolen" },
      { kind: "player", uuid: 42 },
      { kind: "internal", service: "bot" },
      "garbage",
      null,
    ]);
    expect(parsed).toEqual([
      { kind: "player", uuid: "94589c87999b4a7ab3add2ae50711e64" },
      { kind: "account", discord_id: "1218840276405911602", member_id: 23025 },
      { kind: "key", key_id: null, prefix: "e91c0d3a", key_kind: "invalid" },
    ]);
  });

  it("rejects non-arrays and caps the list", () => {
    expect(parseRecent({ kind: "player" })).toEqual([]);
    const many = Array.from({ length: RECENT_LIMIT + 4 }, (_, index) => ({ kind: "name", name: `n${index}` }));
    expect(parseRecent(many)).toHaveLength(RECENT_LIMIT);
  });
});

describe("toStoredRef", () => {
  it("stores only the declared fields of a key ref", () => {
    const smuggled = { kind: "key", key_id: 7, prefix: "e91c0d3a", key_kind: "personal", key: "e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9" } as const;
    expect(toStoredRef(smuggled)).toEqual({ kind: "key", key_id: 7, prefix: "e91c0d3a", key_kind: "personal" });
  });

  it("does not store refs without a page", () => {
    expect(toStoredRef({ kind: "anonymous" })).toBeNull();
    expect(toStoredRef({ kind: "internal", service: "sync" })).toBeNull();
  });
});
