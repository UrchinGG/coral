import type { EntityRef } from "../../api/types/refs";
import type { RequestRow } from "../../api/types/requests";
import type { ChartPoint, ChartSeries } from "../../kit/TimeSeriesChart";
import type { TimelineItem } from "../../kit/Timeline";

export const SAMPLE_PLAYER = "3f9a1c2e7b4d4e8f9a6b1c3d5e7f9012";
export const SAMPLE_ACCOUNT = "1218840276405911602";
export const SAMPLE_KEY = "e91c0d3a-55b2-4e7f-9a18-c3d6f0b2a4e9";

const MINUTE_MS = 60_000;
export const CHART_STEP_MS = MINUTE_MS;
const ROW_SPACING_MS = 1_700;
const CHART_POINTS = 60;
const TABLE_ROWS = 5_000;
const BATCH_SIZE = 450;
const ROUTES = ["/v3/players", "/v3/player/tags", "/v3/player/profile", "/v3/cubelify", "/v3/resolve/{identifier}"] as const;
const METHODS = ["GET", "GET", "GET", "POST", "DELETE"] as const;
const STATUSES = [200, 200, 200, 404, 429, 500] as const;
const CAUSES = { 200: "ok", 404: "not_found", 429: "caller_limit", 500: "internal" } as const;

export const SAMPLE_REFS: readonly EntityRef[] = [
  { kind: "account", discord_id: SAMPLE_ACCOUNT, member_id: 23025 },
  { kind: "player", uuid: SAMPLE_PLAYER },
  { kind: "name", name: "Kestrel_" },
  { kind: "request", request_id: "0199e3f12c3a7b4d8e5f6a7b8c9d0e1f", ts: "2026-10-14T14:32:10.412318Z" },
  { kind: "ip", ip: "198.51.100.23", country: "GB", proxied: false },
  { kind: "ip", ip: "203.0.113.7", country: "US", proxied: true },
  { kind: "key", key_id: null, prefix: "e91c0d3a", key_kind: "invalid" },
  { kind: "key", key_id: 4, prefix: "7b9a4c2e", key_kind: "personal" },
  { kind: "guild", guild_id: "5a8b2c1d9e0f3a4b5c6d7e8f", name: "Example Guild" },
  { kind: "plugin", slug: "example-plugin", name: "Example plugin" },
  { kind: "internal", service: "bot" },
  { kind: "anonymous" },
];

export const CHART_SERIES: readonly ChartSeries[] = [
  { key: "c2xx", label: "2xx", color: "#34d399", opacity: 0.42 },
  { key: "c4xx", label: "4xx", color: "#fbbf24" },
  { key: "c429_caller", label: "429 caller", color: "#fb923c" },
  { key: "c429_upstream", label: "429 upstream", color: "#f472b6" },
  { key: "c5xx", label: "5xx", color: "#f87171" },
];

export const SAMPLE_TIMELINE: readonly TimelineItem[] = [
  { id: "strike", at: "2026-10-14T12:01:00Z", tone: "danger", title: "Strike 1 by the owner", detail: "Tag spam on three players" },
  { id: "tag", at: "2026-10-13T08:40:00Z", tone: "warning", title: "Tagged Kestrel_ as caution", detail: "Reviewed and accepted" },
  { id: "link", at: "2026-10-02T19:12:00Z", tone: "info", title: "Linked a Minecraft account" },
  { id: "join", at: "2026-09-21T10:00:00Z", tone: "ok", title: "Joined Coral" },
];

export function chartPoints(now: number): ChartPoint[] {
  return Array.from({ length: CHART_POINTS }, (_, index) => {
    const wave = Math.sin(index / 6) * 60 + 380;
    return {
      t: new Date(now - (CHART_POINTS - 1 - index) * MINUTE_MS).toISOString(),
      values: {
        c2xx: Math.round(wave),
        c4xx: Math.round(28 + Math.cos(index / 4) * 8),
        c429_caller: index > 40 && index < 48 ? 54 : 6,
        c429_upstream: index % 17 === 0 ? 22 : 2,
        c5xx: index === 33 ? 18 : 1,
      },
      line: Math.round(140 + Math.sin(index / 5) * 40 + (index === 33 ? 600 : 0)),
    };
  });
}

export function sampleRows(now: number, count = TABLE_ROWS, offset = 0): RequestRow[] {
  return Array.from({ length: count }, (_, position) => {
    const index = position + offset;
    const status = STATUSES[index % STATUSES.length];
    const ts = new Date(now - index * ROW_SPACING_MS).toISOString();
    return {
      request_id: `0199e3f1${index.toString(16).padStart(8, "0")}7b4d8e5f6a7b8c9d`,
      ts,
      method: METHODS[index % METHODS.length],
      route: ROUTES[index % ROUTES.length],
      path: ROUTES[index % ROUTES.length].replace("{identifier}", "Kestrel_"),
      query: index % 3 === 0 ? `uuid=${SAMPLE_PLAYER}` : null,
      status,
      outcome_status: status === 429 && index % 2 === 0 ? 200 : null,
      cause: CAUSES[status],
      error: status >= 500 ? "upstream returned an unexpected body" : null,
      latency_ms: 20 + ((index * 37) % 400),
      key_kind: "personal",
      key_prefix: "7b9a4c2e",
      requester: { kind: "account", discord_id: SAMPLE_ACCOUNT, member_id: 23025 },
      actor: null,
      ip: { kind: "ip", ip: index % 5 === 0 ? "203.0.113.7" : "198.51.100.23", country: "GB", proxied: index % 5 === 0 },
      country: "GB",
      user_agent: "ExampleClient/1.0",
      subjects: [{ player: SAMPLE_PLAYER, name: "kestrel_", raw: SAMPLE_PLAYER, form: "undashed" }],
      body_subjects: 0,
    };
  });
}

export function sampleJson(): unknown {
  return {
    uuids: Array.from({ length: BATCH_SIZE }, (_, index) => `${index.toString(16).padStart(8, "0")}7b4d4e8f9a6b1c3d5e7f9012`),
    requested_by: SAMPLE_ACCOUNT,
    player: { uuid: SAMPLE_PLAYER, name: "Kestrel_", stats: { wins: 1204, losses: 311, ratio: 3.87 }, tags: [] },
    flags: { cached: true, stale: false, note: null },
  };
}
