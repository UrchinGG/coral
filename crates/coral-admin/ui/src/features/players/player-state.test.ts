import { describe, expect, it } from "vitest";

import { canonicalPlayerLocation, formLabel, formsParam, parseForms, parseTopWindow, parseWindow, STANDARD_FORMS, toggleForm } from "./player-state";

describe("player lookup forms", () => {
  it("defaults to every form except display names", () => {
    expect(parseForms(null)).toEqual(STANDARD_FORMS);
    expect(STANDARD_FORMS).not.toContain("display");
    expect(formsParam(STANDARD_FORMS)).toBeNull();
  });

  it("keeps listed forms in their display order and drops unknown ones", () => {
    expect(parseForms("body,undashed,bogus")).toEqual(["undashed", "body"]);
    expect(parseForms("bogus")).toEqual(STANDARD_FORMS);
    expect(formsParam(["body", "undashed"])).toBe("undashed,body");
  });

  it("toggles a form on and off but never empties the selection", () => {
    expect(toggleForm(STANDARD_FORMS, "display")).toContain("display");
    expect(toggleForm(["undashed", "body"], "body")).toEqual(["undashed"]);
    expect(toggleForm(["body"], "body")).toEqual(["body"]);
    expect(toggleForm(["body"], "dashed")).toEqual(["dashed", "body"]);
  });
});

describe("form chip labels", () => {
  it("name the current and former spellings", () => {
    expect(formLabel("name", "Kestrel_", [])).toBe('name "Kestrel_"');
    expect(formLabel("former", "Kestrel_", ["kes_tr", "KestrelPvP"])).toBe('former "kes_tr", "KestrelPvP"');
    expect(formLabel("former", null, [])).toBe("former");
    expect(formLabel("body", "Kestrel_", ["kes_tr"])).toBe("body");
  });
});

describe("player locations", () => {
  it("redirects dashed and uppercase UUIDs to the undashed path and keeps the query", () => {
    expect(canonicalPlayerLocation("94589C87-999B-4A7A-B3AD-D2AE50711E64", "?tab=tags")).toBe("/players/94589c87999b4a7ab3add2ae50711e64?tab=tags");
    expect(canonicalPlayerLocation("94589c87999b4a7ab3add2ae50711e64", "")).toBeNull();
    expect(canonicalPlayerLocation("oJugg", "")).toBeNull();
  });

  it("falls back to fourteen days for an unknown window", () => {
    expect(parseWindow("24h")).toBe("24h");
    expect(parseWindow("30d")).toBe("14d");
    expect(parseWindow(null)).toBe("14d");
  });

  it("falls back to the day for an unknown top looked-up window", () => {
    expect(parseTopWindow("1h")).toBe("1h");
    expect(parseTopWindow("7d")).toBe("24h");
    expect(parseTopWindow(null)).toBe("24h");
  });
});
