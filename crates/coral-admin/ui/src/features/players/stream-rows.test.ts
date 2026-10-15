import { describe, expect, it } from "vitest";

import type { PlayerRequestRow } from "../../api/types/players";
import { matchedValue, serviceOf, viaLabel } from "./stream-rows";

const PLAYER = "3f9a1c2e7b4d4e8f9a6b1c3d5e7f9012";

function row(overrides: Partial<PlayerRequestRow>): PlayerRequestRow {
  return {
    request_id: "01a13d38170e77bb809460883c22e7d7",
    ts: "2026-10-15T01:39:17.014925Z",
    method: "GET",
    route: "/v3/player/tags",
    path: "/v3/player/tags",
    query: null,
    status: 200,
    outcome_status: null,
    cause: "ok",
    error: null,
    latency_ms: 4,
    key_kind: "personal",
    key_prefix: "fd9bb63f",
    requester: { kind: "account", discord_id: "300000000000001830", member_id: 1830 },
    actor: null,
    ip: null,
    country: null,
    user_agent: null,
    subjects: [],
    body_subjects: 0,
    match: "uuid",
    matched_form: "undashed",
    ...overrides,
  };
}

describe("stream rows", () => {
  it("shows the raw text of the subject that matched", () => {
    const dashed = row({ matched_form: "dashed", subjects: [{ player: PLAYER, name: null, raw: "3f9a1c2e-7b4d-4e8f-9a6b-1c3d5e7f9012", form: "dashed" }] });
    expect(matchedValue(dashed, PLAYER, [])).toBe("3f9a1c2e-7b4d-4e8f-9a6b-1c3d5e7f9012");
    const former = row({ matched_form: "former", match: "name_only", subjects: [{ player: null, name: "kes_tr", raw: "Kes_TR", form: "name" }] });
    expect(matchedValue(former, PLAYER, ["kestrel_", "kes_tr"])).toBe("Kes_TR");
    expect(matchedValue(former, PLAYER, ["kestrel_"])).toBeNull();
  });

  it("has no listed value for batch bodies", () => {
    expect(matchedValue(row({ matched_form: "body", body_subjects: 50 }), PLAYER, [])).toBeNull();
  });

  it("names the service an internal request came through", () => {
    expect(serviceOf(row({ requester: { kind: "internal", service: "bot" } }))).toBe("bot");
    expect(serviceOf(row({}))).toBeNull();
    expect(viaLabel("bot")).toBe("via bot");
    expect(viaLabel("verify")).toBe("via mc-verify");
  });
});
