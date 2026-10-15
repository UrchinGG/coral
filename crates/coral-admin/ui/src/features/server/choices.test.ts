import { describe, expect, it } from "vitest";

import type { DiscordRole } from "../../api/types/platform";
import { roleChoices } from "./choices";

const ROLE: DiscordRole = { id: "1200000000000000010", name: "Reviewers", color: null, position: 4, managed: false, assignable: true, bot_assignable: false };

describe("role choices", () => {
  it("judges each role by the bot that will hand it out", () => {
    expect(roleChoices([ROLE])?.[0]).toMatchObject({ disabled: false, hint: undefined });
    expect(roleChoices([ROLE], { manager: "bot" })?.[0]).toMatchObject({ disabled: true, hint: "not manageable" });
    expect(roleChoices([{ ...ROLE, bot_assignable: null }], { manager: "bot" })?.[0]).toMatchObject({ disabled: true, hint: "bot not configured" });
  });

  it("marks roles that already carry a rule", () => {
    expect(roleChoices([ROLE], { taken: new Set([ROLE.id]) })?.[0]).toMatchObject({ disabled: true, hint: "has rule" });
  });

  it("waits for the roles before offering any", () => {
    expect(roleChoices(undefined)).toBeUndefined();
  });
});
