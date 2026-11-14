import { describe, expect, it } from "vitest";

import { evalCondition } from "./expr";

describe("evalCondition", () => {
  it("treats a missing field as neither above nor below anything", () => {
    const ctx = { rbw: { elo: 800 } };
    for (const condition of ["rbw.wins < 10", "rbw.wins > 10", "rbw.wins <= 0", "rbw.wins >= 0", "0 < rbw.wins"]) {
      expect(evalCondition(condition, ctx), condition).toBe(false);
    }
    expect(evalCondition("rbw.elo < 1000", ctx)).toBe(true);
    expect(evalCondition("rbw.wins == 0", ctx)).toBe(false);
    expect(evalCondition("rbw.wins != 0", ctx)).toBe(true);
  });
});
