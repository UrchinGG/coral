import { describe, expect, it } from "vitest";

import { conditionOf, type BuilderRow } from "./condition";

const LEVEL: BuilderRow = { joiner: "and", field: "achievements.bedwars_level", op: ">=", value: "500" };

describe("conditionOf", () => {
  it("joins clauses with each row's own joiner and quotes text values", () => {
    const rows: BuilderRow[] = [LEVEL, { joiner: "or", field: " blacklist.tag ", op: "==", value: " sniper " }];
    expect(conditionOf(rows)).toBe('achievements.bedwars_level >= 500 or blacklist.tag == "sniper"');
  });

  it("is empty until every row has a field and a value", () => {
    expect(conditionOf([LEVEL, { joiner: "and", field: "coral.access", op: ">", value: "" }])).toBe("");
    expect(conditionOf([{ ...LEVEL, field: " " }])).toBe("");
  });
});
