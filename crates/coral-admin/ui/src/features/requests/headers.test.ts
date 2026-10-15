import { describe, expect, it } from "vitest";

import type { RequestFull } from "../../api/types/request-full";
import { groupHeaders, headerGroup, rawHeaders } from "./headers";
import { curlFor, requestJson, storedBytes } from "./request-copy";

describe("groupHeaders", () => {
  it("files every header under one group and sorts each group by name", () => {
    const grouped = groupHeaders({
      "x-forwarded-for": "203.0.113.9",
      "user-agent": "curl/8.5.0",
      "cf-ray": "8d1",
      "x-api-key": "k",
      accept: "*/*",
      "cf-ipcountry": "NL",
      "x-custom": "1",
    });
    expect(grouped).toEqual([
      { group: "credentials", headers: [["x-api-key", "k"]] },
      { group: "client", headers: [["accept", "*/*"], ["user-agent", "curl/8.5.0"]] },
      { group: "cloudflare", headers: [["cf-ipcountry", "NL"], ["cf-ray", "8d1"]] },
      { group: "proxy", headers: [["x-forwarded-for", "203.0.113.9"]] },
      { group: "other", headers: [["x-custom", "1"]] },
    ]);
  });

  it("recognizes headers whatever their case", () => {
    expect(headerGroup("X-Starfish-Session")).toBe("credentials");
    expect(headerGroup("Content-Type")).toBe("client");
    expect(headerGroup("CDN-Loop")).toBe("cloudflare");
  });

  it("prints raw headers one per line", () => {
    expect(rawHeaders({ b: "2", a: "1" })).toBe("a: 1\nb: 2");
  });
});

const FULL: RequestFull = {
  request: {
    request_id: "0199e4a17c2b7d108a3e2f1d9c4b5a60",
    ts: "2026-10-14T21:41:07.556439Z",
    method: "POST",
    route: "/v3/players",
    path: "/v3/players",
    query: null,
    status: 200,
    outcome_status: null,
    cause: "ok",
    error: null,
    latency_ms: 12,
    key_kind: "personal",
    key_prefix: "1a2b3c4d",
    requester: { kind: "anonymous" },
    actor: null,
    ip: null,
    country: null,
    user_agent: null,
    subjects: [],
    body_subjects: 0,
  },
  url: "https://api.urchin.gg/v3/players",
  query_params: [],
  headers: { "content-length": "11", "x-api-key": "k" },
  body: '{"uuids":[]}',
  body_bytes: 12,
  body_complete: true,
  body_json: { uuids: [] },
  server_error: null,
  detail_state: "present",
  subjects: [],
  caller: { account: null, actor: null, key: null, starfish_user: null, plugin: null },
  neighbors: { prev_same_requester: null, next_same_requester: null, same_player_5m: 0 },
  labels: { discord: {}, minecraft: {}, pending: { discord: [], minecraft: [] } },
};

describe("request copies", () => {
  const full = FULL;

  it("replays the stored headers and body but lets curl count the length", () => {
    const { command } = curlFor(full);
    expect(command).toContain("--header 'x-api-key: k'");
    expect(command).not.toContain("content-length");
    expect(command.startsWith(`printf '%s' '{"uuids":[]}' | curl`)).toBe(true);
  });

  it("copies the request without its label cache", () => {
    const json = JSON.parse(requestJson(full));
    expect(json.labels).toBeUndefined();
    expect(json.url).toBe(full.url);
  });

  it("counts stored bytes in UTF-8", () => {
    expect(storedBytes("§e")).toBe(3);
  });
});
