import { describe, expect, it } from "vitest";

import type { SearchHit } from "../api/types/search";
import { classify } from "./classify";
import { decisiveHits, enterHit, enterTargetOf, failureSummary, hintItems, hitPath, mergeHits, searchActions } from "./results";

const ADMIN = "https://admin.urchin.gg";
const KEY = "e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9";

function hit(partial: Partial<SearchHit> & Pick<SearchHit, "ref" | "score">): SearchHit {
  return { exact: false, reason: "", account_id: null, tab: null, ...partial };
}

const PLAYER = { kind: "player", uuid: "94589c87999b4a7ab3add2ae50711e64" } as const;
const ACCOUNT = { kind: "account", discord_id: "1218840276405911602", member_id: 23025 } as const;
const OTHER_ACCOUNT = { kind: "account", discord_id: "1203948576102938475", member_id: 1 } as const;

function enterPath(hits: readonly SearchHit[]): string | null {
  const target = enterHit(hits);
  return target === null ? null : hitPath(target);
}

describe("enterHit", () => {
  it("opens an exact hit that leads the next by at least 15 points", () => {
    expect(enterPath([hit({ ref: PLAYER, score: 90, exact: true }), hit({ ref: OTHER_ACCOUNT, score: 75 })])).toBe(`/players/${PLAYER.uuid}`);
  });

  it("opens /search when the lead is too small and the hits disagree", () => {
    expect(enterPath([hit({ ref: PLAYER, score: 90, exact: true }), hit({ ref: OTHER_ACCOUNT, score: 85 })])).toBeNull();
  });

  it("opens the shared account when every top-tier hit resolves to it", () => {
    const hits = [
      hit({ ref: PLAYER, score: 90, exact: true, account_id: ACCOUNT.discord_id }),
      hit({ ref: ACCOUNT, score: 85, exact: true, account_id: ACCOUNT.discord_id }),
      hit({ ref: OTHER_ACCOUNT, score: 60, account_id: OTHER_ACCOUNT.discord_id }),
    ];
    expect(enterPath(hits)).toBe(`/accounts/${ACCOUNT.discord_id}`);
  });

  it("keeps the tab of a hit", () => {
    expect(enterPath([hit({ ref: ACCOUNT, score: 95, exact: true, tab: "lookups" })])).toBe(`/accounts/${ACCOUNT.discord_id}?tab=lookups`);
  });

  it("returns null without hits", () => {
    expect(enterPath([])).toBeNull();
  });

  it("opens the shared account's own hit so the account itself is remembered", () => {
    const own = hit({ ref: ACCOUNT, score: 85, exact: true, account_id: ACCOUNT.discord_id, reason: "Discord username" });
    const hits = [hit({ ref: PLAYER, score: 90, exact: true, account_id: ACCOUNT.discord_id }), own];
    expect(enterHit(hits)).toBe(own);
  });
});

describe("enterTargetOf", () => {
  it("does nothing for empty input", () => {
    expect(enterTargetOf("", classify("", ADMIN), null)).toBeNull();
  });

  it("opens a pasted admin URL without waiting for hits", () => {
    expect(enterTargetOf(`${ADMIN}/players/x?tab=tags`, classify(`${ADMIN}/players/x?tab=tags`, ADMIN), null)).toEqual({ type: "path", path: "/players/x?tab=tags", state: null });
  });

  it("opens /search with the text in history state when the search has not settled", () => {
    expect(enterTargetOf(KEY, classify(KEY, ADMIN), null)).toEqual({ type: "path", path: "/search", state: { coral: { search: KEY } } });
  });

  it("opens the decisive hit once the search has settled", () => {
    expect(enterTargetOf("Kestrel_", classify("Kestrel_", ADMIN), [hit({ ref: PLAYER, score: 90, exact: true })])).toMatchObject({ type: "entity", ref: PLAYER, tab: null });
  });
});

describe("mergeHits", () => {
  it("keeps one hit per entity and tab, the higher score winning", () => {
    const former = hit({ ref: PLAYER, score: 70, reason: "Former owner of the name" });
    const mojang = hit({ ref: PLAYER, score: 90, exact: true, reason: "Mojang: Kestrel_ is this player's current name" });
    const lookups = hit({ ref: ACCOUNT, score: 70, tab: "lookups" });
    const account = hit({ ref: ACCOUNT, score: 95, exact: true });
    expect(mergeHits([former, account, lookups], [mojang])).toEqual([mojang, account, lookups]);
  });
});

describe("failed lookups", () => {
  const failures = [{ lookup: "plugin", code: "timeout", message: "canceling statement due to statement timeout" }];

  it("are summarised for the results", () => {
    expect(failureSummary([])).toBeNull();
    expect(failureSummary(failures)).toBe("Some lookups failed, so these results may be incomplete: plugin (timeout: canceling statement due to statement timeout).");
  });

  it("make the hits too uncertain for Enter to open one", () => {
    const hits = [hit({ ref: PLAYER, score: 95, exact: true })];
    expect(decisiveHits(hits, [], false)).toBe(hits);
    expect(decisiveHits(hits, failures, false)).toBeNull();
    expect(enterTargetOf("Kestrel_", classify("Kestrel_", ADMIN), decisiveHits(hits, failures, false))).toEqual({ type: "path", path: "/search", state: { coral: { search: "Kestrel_" } } });
  });
});

describe("a pending Mojang lookup", () => {
  it("keeps Enter from opening a local hit the Mojang answer could outrank", () => {
    const naming = [hit({ ref: { kind: "name", name: "notch" }, score: 65, exact: true })];
    expect(decisiveHits(naming, [], true)).toBeNull();
    expect(enterTargetOf("Notch", classify("Notch", ADMIN), decisiveHits(naming, [], true))).toEqual({ type: "path", path: "/search", state: { coral: { search: "Notch" } } });
    expect(decisiveHits(naming, [], false)).toBe(naming);
  });
});

describe("hintItems", () => {
  it("never puts a dashed UUID that could be a key into a URL", () => {
    const items = hintItems(classify("e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9", ADMIN));
    expect(items).toEqual([]);
  });

  it("offers a request for a dashed UUIDv7 with its timestamp", () => {
    const items = hintItems(classify("0199e3f1-2c3a-7b4d-8e5f-6a7b8c9d0e1f", ADMIN));
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "request", path: expect.stringMatching(/^\/requests\/0199e3f12c3a7b4d8e5f6a7b8c9d0e1f\?t=/) });
  });

  it("offers the player from a pasted API URL and never its key", () => {
    const items = hintItems(classify(`https://api.urchin.gg/v3/player/tags?uuid=94589c87999b4a7ab3add2ae50711e64&key=${KEY}`, ADMIN));
    const paths = items.map((item) => (item.type === "link" ? item.path : ""));
    expect(paths).toEqual(["/players/94589c87999b4a7ab3add2ae50711e64"]);
    expect(JSON.stringify(items)).not.toContain(KEY.replaceAll("-", "").slice(8));
  });

  it("offers requests naming a name from a pasted API URL", () => {
    const items = hintItems(classify(`https://api.urchin.gg/v3/player/tags?name=Kestrel_&key=${KEY}`, ADMIN));
    expect(items.map((item) => (item.type === "link" ? item.path : ""))).toEqual(["/requests?player=Kestrel_&include_display=1"]);
    expect(JSON.stringify(items)).not.toContain(KEY.slice(9));
  });

  it("filters hints by the forced operator", () => {
    const items = hintItems(classify("k:e91c0d3a", ADMIN));
    expect(items.map((item) => (item.type === "link" ? item.kind : ""))).toEqual(["key"]);
  });
});

describe("searchActions", () => {
  it("keeps free text out of the URL", () => {
    const [requestText, all] = searchActions(classify("timeout from cloudflare", ADMIN));
    expect(requestText).toMatchObject({ path: "/requests", state: { coral: { q: "timeout from cloudflare" } } });
    expect(all).toMatchObject({ path: "/search", state: { coral: { search: "timeout from cloudflare" } } });
  });
});
