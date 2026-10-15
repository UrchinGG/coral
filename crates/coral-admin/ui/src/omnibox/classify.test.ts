import { describe, expect, it } from "vitest";

import { classify, describeClassification, normalizeQuery } from "./classify";

const ADMIN = "https://admin.urchin.gg";
const UNDASHED = "94589c87999b4a7ab3add2ae50711e64";
const DASHED = "94589c87-999b-4a7a-b3ad-d2ae50711e64";

describe("normalizeQuery", () => {
  it("trims and strips quotes and mentions", () => {
    expect(normalizeQuery("  'ojugg'  ")).toBe("ojugg");
    expect(normalizeQuery("<@1218840276405911602>")).toBe("1218840276405911602");
    expect(normalizeQuery("<@!1218840276405911602>")).toBe("1218840276405911602");
    expect(normalizeQuery('"`Kestrel_`"')).toBe("Kestrel_");
  });
});

describe("classify", () => {
  it.each([
    ["1218840276405911602", ["discord_id"]],
    ["23025", ["member_id", "name"]],
    ["7", ["member_id", "short_name"]],
    [UNDASHED, ["uuid_undashed"]],
    [DASHED, ["uuid_dashed"]],
    ["5a8b2c1d9e0f3a4b5c6d7e8f", ["guild_id"]],
    ["e91c0d3a", ["key_prefix", "name"]],
    ["40213977", ["member_id", "key_prefix", "name"]],
    ["198.51.100.23", ["ipv4"]],
    ["2001:db8::1", ["ipv6"]],
    ["Kestrel_", ["name"]],
    ["ab", ["short_name"]],
    ["kestrel tr", ["text"]],
  ])("classifies %s", (input, shapes) => {
    expect(classify(input, ADMIN).shapes).toEqual(shapes);
  });

  it("applies operators and strips them from the text", () => {
    const result = classify("p: 'Kestrel_'", ADMIN);
    expect(result.operator).toBe("player");
    expect(result.text).toBe("Kestrel_");
    expect(classify("pl:urchin", ADMIN).operator).toBe("plugin");
    expect(classify("ip:198.51.100.23", ADMIN).operator).toBe("ip");
    expect(classify("n:ab", ADMIN)).toMatchObject({ operator: "name", shapes: ["short_name"] });
  });

  it("navigates admin URLs directly", () => {
    expect(classify(`${ADMIN}/accounts/1218840276405911602?tab=network`, ADMIN).url).toEqual({
      kind: "admin",
      path: "/accounts/1218840276405911602?tab=network",
    });
  });

  it("parses API URLs into identifiers and flags a carried key without exposing it", () => {
    const result = classify(`https://api.urchin.gg/v3/player/tags?uuid=${DASHED}&key=1f0e3d5c-7b9a-4c2e-8d6f-a0b1c2d3e4f5`, ADMIN);
    expect(result.url).toEqual({ kind: "api", identifiers: [UNDASHED], carriesKey: true });
    expect(describeClassification(result)).toBe("API URL · 1 identifier · carries a key");
  });

  it("reads names from resolve paths", () => {
    expect(classify("https://api.urchin.gg/v3/resolve/Kestrel_", ADMIN).url).toEqual({
      kind: "api",
      identifiers: ["Kestrel_"],
      carriesKey: false,
    });
  });

  it("reads no names from session paths", () => {
    expect(classify(`https://api.urchin.gg/v3/player/sessions/daily?player=${UNDASHED}`, ADMIN).url).toEqual({
      kind: "api",
      identifiers: [UNDASHED],
      carriesKey: false,
    });
    expect(classify("https://api.urchin.gg/v3/player/sessions/markers/bedwars_win", ADMIN).url).toEqual({ kind: "api", identifiers: [], carriesKey: false });
  });

  it("describes ambiguous shapes", () => {
    expect(describeClassification(classify("e91c0d3a", ADMIN))).toBe("key prefix or name");
    expect(describeClassification(classify("k:", ADMIN))).toBe("type forced: key");
  });
});
