import { describe, expect, it } from "vitest";

import { accessLabel, permissionLabels } from "./vocabulary";

describe("accessLabel", () => {
  it("names each rank and rounds unknown levels down", () => {
    expect(accessLabel(0)).toBe("Default");
    expect(accessLabel(3)).toBe("Moderator");
    expect(accessLabel(1)).toBe("Default");
    expect(accessLabel(9)).toBe("Owner");
  });
});

describe("permissionLabels", () => {
  it("lists granted developer key permissions", () => {
    expect(permissionLabels(0)).toBe("none");
    expect(permissionLabels(1 | 4)).toBe("Player data, All sessions");
  });
});
