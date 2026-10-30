import { describe, expect, it } from "vitest";

import type { RequesterGroup } from "../../api/types/request-explorer";
import {
  EMPTY_FILTER,
  encodeRequester,
  explorerViewFrom,
  filterFromParams,
  groupQueryOf,
  groupGaps,
  groupSelection,
  groupSortFrom,
  histogramGaps,
  parseRequester,
  requesterFromText,
  requestQueryOf,
  windowBounds,
  withFilterParams,
  type ExplorerFilter,
} from "./explorer-filter";

const ANCHOR = Date.parse("2026-10-14T12:00:00.000Z");
const MEMBER_ACCOUNT = { kind: "account", discord_id: "354598337325826048", member_id: 23025 } as const;

function group(overrides: Partial<RequesterGroup>): RequesterGroup {
  return {
    requester: MEMBER_ACCOUNT,
    actor: null,
    key_kind: "personal",
    service: "unknown",
    key_prefix: null,
    requests: 1,
    rate_limited: 0,
    errors: 0,
    subjects: 0,
    first: "2026-10-14T10:00:00.000000Z",
    last: "2026-10-14T11:00:00.000000Z",
    ...overrides,
  };
}

describe("explorer URL state", () => {
  it("round-trips every filter through the address bar", () => {
    const filter: ExplorerFilter = {
      range: { kind: "custom", from: "2026-10-14T10:00:00.000Z", to: "2026-10-14T11:00:00.000Z" },
      route: "/v3/resolve/{identifier}",
      method: "GET",
      status: 429,
      statusClass: "rate_limited",
      cause: "upstream_limit",
      keyKind: "invalid",
      requester: { kind: "key", prefix: "1a2b3c4d", keyKind: "invalid" },
      player: "oJugg",
      includeDisplay: true,
      ip: "2001:db8::1",
      country: "NL",
    };
    const params = withFilterParams(new URLSearchParams("view=groups&r=abc"), filter);
    expect(params.get("view")).toBe("groups");
    expect(params.get("r")).toBe("abc");
    expect(filterFromParams(params)).toEqual(filter);
  });

  it("leaves the default window and unset filters out of the URL", () => {
    expect(withFilterParams(new URLSearchParams(), EMPTY_FILTER).toString()).toBe("");
    expect(filterFromParams(new URLSearchParams("window=bogus&class=3xx&status=42"))).toEqual(EMPTY_FILTER);
  });

  it("ignores a custom range that ends before it starts", () => {
    const params = new URLSearchParams({ from: "2026-10-14T11:00:00Z", to: "2026-10-14T10:00:00Z" });
    expect(filterFromParams(params).range).toEqual({ kind: "window", window: "1h" });
  });
});

describe("requester filters", () => {
  it("encode members, actors and key prefixes", () => {
    for (const requester of [
      { kind: "member", memberId: 23025, discordId: "354598337325826048" },
      { kind: "member", memberId: 23025, discordId: null },
      { kind: "actor", discordId: "354598337325826048" },
      { kind: "player", uuid: "2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21" },
      { kind: "key", prefix: "1a2b3c4d", keyKind: null },
    ] as const) {
      expect(parseRequester(encodeRequester(requester))).toEqual(requester);
    }
    expect(parseRequester("key:short")).toBeNull();
    expect(parseRequester("nobody:1")).toBeNull();
  });

  it("read typed snowflakes, member ids and key prefixes", () => {
    expect(requesterFromText(" 354598337325826048 ")).toEqual({ kind: "actor", discordId: "354598337325826048" });
    expect(requesterFromText("23025")).toEqual({ kind: "member", memberId: 23025, discordId: null });
    expect(requesterFromText("2C1F6A0E-7D4B-4F3A-9E1D-5B8C0A6F3E21")).toEqual({ kind: "player", uuid: "2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21" });
    expect(requesterFromText("1a2b3c4d-5e6f")).toEqual({ kind: "key", prefix: "1a2b3c4d", keyKind: null });
    expect(requesterFromText("abc")).toBeNull();
  });

  it("narrow the list to the clicked requester group", () => {
    expect(groupSelection(group({}))).toEqual({ requester: { kind: "member", memberId: 23025, discordId: "354598337325826048" }, keyKind: null });
    expect(groupSelection(group({ requester: { kind: "key", key_id: null, prefix: "1a2b3c4d", key_kind: "invalid" }, key_kind: "invalid" }))).toEqual({
      requester: { kind: "key", prefix: "1a2b3c4d", keyKind: "invalid" },
      keyKind: "invalid",
    });
    expect(
      groupSelection(group({ requester: { kind: "internal", service: "bot" }, actor: { kind: "account", discord_id: "611829304758129664", member_id: null }, key_kind: "internal" })),
    ).toEqual({ requester: { kind: "actor", discordId: "611829304758129664" }, keyKind: "internal" });
    expect(groupSelection(group({ requester: { kind: "anonymous" }, key_kind: "none" }))).toEqual({ requester: null, keyKind: "none" });
  });
});

describe("query bodies", () => {
  const filter: ExplorerFilter = { ...EMPTY_FILTER, requester: { kind: "member", memberId: 23025, discordId: null }, player: "oJugg", includeDisplay: true };

  it("anchor relative windows and leave the top open for the list", () => {
    expect(windowBounds({ kind: "window", window: "6h" }, ANCHOR)).toEqual({ from: "2026-10-14T06:00:00.000Z", to: null });
  });

  it("send only the fields that are set", () => {
    const bounds = windowBounds(EMPTY_FILTER.range, ANCHOR);
    expect(requestQueryOf(filter, "  timeout ", bounds)).toEqual({
      from: "2026-10-14T11:00:00.000Z",
      member: 23025,
      player: "oJugg",
      include_display: true,
      q: "timeout",
    });
    expect(requestQueryOf(EMPTY_FILTER, "", bounds)).toEqual({ from: "2026-10-14T11:00:00.000Z" });
  });

  it("close the window for requester groups and keep only rollup dimensions", () => {
    const bounds = windowBounds(EMPTY_FILTER.range, ANCHOR);
    expect(groupQueryOf(filter, bounds, ANCHOR, "errors")).toEqual({
      from: "2026-10-14T11:00:00.000Z",
      to: "2026-10-14T12:00:00.000Z",
      member: 23025,
      sort: "errors",
    });
    expect(groupGaps(filter, "x")).toEqual(["player", "text"]);
  });

  it("refuse groups and the histogram a key prefix, which the rollups keep only for unattributed keys", () => {
    const byKey: ExplorerFilter = { ...EMPTY_FILTER, requester: { kind: "key", prefix: "5c48b73b", keyKind: null } };
    expect(groupGaps(byKey, "")).toEqual(["key prefix"]);
    expect(histogramGaps(byKey, "")).toEqual(["key prefix"]);
    expect(groupGaps(EMPTY_FILTER, " ")).toEqual([]);
  });

  it("filter the list by a Minecraft account, which the rollups do not carry", () => {
    const byPlayer: ExplorerFilter = { ...EMPTY_FILTER, requester: { kind: "player", uuid: "2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21" } };
    expect(requestQueryOf(byPlayer, "", windowBounds(EMPTY_FILTER.range, ANCHOR))).toEqual({ from: "2026-10-14T11:00:00.000Z", actor: "2c1f6a0e7d4b4f3a9e1d5b8c0a6f3e21" });
    expect(groupGaps(byPlayer, "")).toEqual(["Minecraft account"]);
    expect(histogramGaps(byPlayer, "")).toEqual(["Minecraft account"]);
  });

  it("read the view and the group order from the URL, defaulting to the list by requests", () => {
    expect(explorerViewFrom(new URLSearchParams("view=groups"))).toBe("groups");
    expect(explorerViewFrom(new URLSearchParams("view=chart"))).toBe("list");
    expect(groupSortFrom(new URLSearchParams("sort=errors"))).toBe("errors");
    expect(groupSortFrom(new URLSearchParams("sort=latency"))).toBe("requests");
  });
});
