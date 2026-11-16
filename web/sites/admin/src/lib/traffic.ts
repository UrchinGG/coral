import { load } from "@/lib/api";
import { legend, stackedArea, stackedBars } from "@/lib/chart";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { pager, type CursorPage } from "@/lib/pager";
import { refName, type Ref } from "@/lib/refs";
import {
  methodBadge,
  requestHead,
  requestRow,
  type RequestRow,
} from "@/lib/requests";
import { moment } from "@/lib/time";
import { API } from "@/lib/paths";

export const WINDOWS: [string, string, number][] = [
  ["1h", "1 h", 3_600_000],
  ["6h", "6 h", 21_600_000],
  ["24h", "24 h", 86_400_000],
  ["7d", "7 d", 604_800_000],
  ["30d", "30 d", 2_592_000_000],
];

export interface Filters {
  window: string;
  route: string | null;
  method: string | null;
  class: string | null;
  member: number | null;
  key_prefix: string | null;
  requester: string | null;
  q: string;
  ip: string;
}

interface Totals {
  requests: number;
  errors: number;
  rate_limited: { caller: number; upstream: number };
  p95: number | null;
  rpm_now: number | null;
}

interface SeriesPoint {
  t: string;
  c2xx: number;
  c4xx: number;
  c429_caller: number;
  c429_upstream: number;
  c5xx: number;
}

export interface RouteTraffic {
  route: string | null;
  method: string;
  requests: number;
  errors: number;
  error_rate: number;
  rate_limited: number;
  p95: number | null;
}

interface Caller {
  requester: Ref;
  key_kind: string;
  key_prefix: string | null;
  requests: number;
  rate_limited: number;
  errors: number;
}

interface Upstream {
  series: { t: string; requests: number; errors: number }[];
  snapshots: { t: string; first: number; delta: number }[];
  capacity: { used: number; limit: number } | null;
  step_s: number;
}

const HYPIXEL_WINDOW_S = 300;
export const UNMATCHED = "unmatched";

export const pickedRoutes = (filters: Filters) =>
  filters.route ? filters.route.split(",") : [];

const CLASSES: [keyof SeriesPoint, string, string][] = [
  ["c2xx", "Answered", "#3a9a5b"],
  ["c4xx", "Client error", "#6a6d74"],
  ["c429_caller", "Rate limited", "#e5c74a"],
  ["c429_upstream", "Hypixel limited", "#e8a23a"],
  ["c5xx", "Server error", "#e66767"],
];

const query = (filters: Filters, keys: (keyof Filters)[]) => {
  const params = new URLSearchParams();
  for (const key of keys) {
    const value = filters[key];
    if (value !== null && value !== "") params.set(key, String(value));
  }
  return params;
};

async function labelsOf(discord: string[]): Promise<Labels> {
  const response = await fetch(`${API}/labels`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ discord, minecraft: [] }),
  });
  return response.ok ? response.json() : { discord: {}, minecraft: {} };
}

const stat = (label: string, value: string, className = "") =>
  el(
    "div",
    { className: "stat" },
    el("span", { className: "label", textContent: label }),
    el("b", { className, textContent: value }),
  );

export async function summary(filters: Filters) {
  const data = await load<{ totals: Totals }>(
    `/traffic/summary?${query(filters, ["window"])}`,
  );
  const totals = data.totals;
  const errorRate = totals.requests
    ? (totals.errors / totals.requests) * 100
    : 0;
  return el(
    "div",
    { className: "stats" },
    stat("Requests", FORMATS.short(totals.requests)),
    stat("Error Rate", `${errorRate.toFixed(1)}%`),
    stat(
      "Rate Limited",
      FORMATS.number(totals.rate_limited.caller + totals.rate_limited.upstream),
    ),
    stat("P95", totals.p95 === null ? "—" : `${totals.p95} ms`),
    stat(
      "Per Minute Now",
      totals.rpm_now === null ? "—" : FORMATS.number(totals.rpm_now),
    ),
  );
}

export async function series(filters: Filters) {
  const data = await load<{ points: SeriesPoint[] }>(
    `/traffic/series?${query(filters, ["window", "route", "class", "member"])}`,
  );
  const bars = stackedBars(
    CLASSES.map(([key, label, color]) => ({
      label,
      color,
      values: data.points.map((point) => point[key] as number),
    })),
    data.points.map((point) => moment(point.t)),
  );
  return el(
    "section",
    { className: "panel" },
    el("h2", {
      className: "panel-title",
      textContent: filters.route
        ? `Requests to ${pickedRoutes(filters).join(", ")}`
        : "Requests by status class",
    }),
    legend(CLASSES.map(([, label, color]) => ({ label, color }))),
    bars,
  );
}

export async function routes(
  filters: Filters,
  search: string,
  pick: (route: RouteTraffic) => void,
) {
  const data = {
    routes: await load<RouteTraffic[]>(
      `/traffic/routes?${query(filters, ["window"])}`,
    ),
  };
  const text = search.toLowerCase();
  const head = el(
    "div",
    { className: "table-row route-row head" },
    ...["Route", "Requests", "Errors", "429", "P95"].map((label, index) =>
      el("span", { className: index ? "num" : "", textContent: label }),
    ),
  );
  const rows = data.routes
    .filter(
      (route) =>
        !text ||
        `${route.method} ${route.route ?? "unmatched"}`
          .toLowerCase()
          .includes(text),
    )
    .map((route) => {
      const line = el(
        "button",
        { type: "button", className: "table-row route-row" },
        el(
          "span",
          { className: "route" },
          methodBadge(route.method),
          route.route ?? "Unmatched paths",
        ),
        el("span", {
          className: "num",
          textContent: FORMATS.short(route.requests),
        }),
        el("span", {
          className: "num",
          textContent: `${(route.error_rate * 100).toFixed(1)}%`,
        }),
        el("span", {
          className: "num",
          textContent: FORMATS.number(route.rate_limited),
        }),
        el("span", {
          className: "num",
          textContent: route.p95 === null ? "" : `${route.p95} ms`,
        }),
      );
      line.setAttribute(
        "aria-pressed",
        String(
          pickedRoutes(filters).includes(route.route ?? UNMATCHED) &&
            (filters.method === null || route.method === filters.method),
        ),
      );
      line.addEventListener("click", () => pick(route));
      return line;
    });
  return el("div", { className: "table" }, head, ...rows);
}

export async function callers(
  filters: Filters,
  pick: (caller: Caller, name: string) => void,
) {
  const data = await load<{ callers: Caller[] }>(
    `/traffic/callers?${query(filters, ["window", "route", "method", "class"])}`,
  );
  const labels = await labelsOf(
    data.callers.flatMap((caller) =>
      caller.requester.kind === "account" ? [caller.requester.discord_id] : [],
    ),
  );
  const head = el(
    "div",
    { className: "table-row caller head" },
    ...["Requester", "Via", "Requests", "429", "Errors"].map((label, index) =>
      el("span", { className: index > 1 ? "num" : "", textContent: label }),
    ),
  );
  const rows = data.callers.map((caller) => {
    const name = refName(caller.requester, labels);
    const line = el(
      "button",
      { type: "button", className: "table-row caller" },
      el("span", { className: "strong", textContent: name }),
      el("span", { textContent: caller.key_kind }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(caller.requests),
      }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(caller.rate_limited),
      }),
      el("span", {
        className: "num",
        textContent: FORMATS.number(caller.errors),
      }),
    );
    line.setAttribute("aria-pressed", String(name === filters.requester));
    line.addEventListener("click", () => pick(caller, name));
    return line;
  });
  return el("div", { className: "table" }, head, ...rows);
}

export function requests(filters: Filters) {
  const span =
    WINDOWS.find(([key]) => key === filters.window)?.[2] ?? WINDOWS[2][2];
  const body: Record<string, string | number> = {
    ...Object.fromEntries(
      query(filters, ["route", "method", "class", "key_prefix", "q", "ip"]),
    ),
    from: new Date(Date.now() - span).toISOString(),
  };
  if (filters.member !== null) body.member = filters.member;
  const fetchPage = async (cursor: string | null) => {
    const response = await fetch(`${API}/requests/query`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(cursor ? { ...body, cursor } : body),
    });
    if (!response.ok)
      throw new Error(
        (await response.json().catch(() => null))?.error?.message ??
          `Requests answered ${response.status}`,
      );
    return (await response.json()) as CursorPage<RequestRow>;
  };
  return el(
    "div",
    { className: "table" },
    requestHead(),
    pager<RequestRow>(fetchPage, requestRow, "No request matches."),
  );
}

const UPSTREAM_MODES: [string, string][] = [
  ["calls", "Calls"],
  ["outcome", "Outcome"],
];

export async function upstream(
  window: string,
  mode: string,
  pickMode: (mode: string) => void,
) {
  const data = await load<Upstream>(`/traffic/upstream?window=${window}`);
  const limit = data.capacity
    ? (data.capacity.limit * data.step_s) / HYPIXEL_WINDOW_S
    : null;
  const calls = data.series.map((point) => point.requests);
  const errors = data.series.map((point) => point.errors);
  const first = data.snapshots.map((point) => point.first);
  const changed = data.snapshots.map((point) => point.delta);
  const unchanged = calls.map((value, index) =>
    Math.max(
      0,
      value - errors[index] - (first[index] ?? 0) - (changed[index] ?? 0),
    ),
  );
  const errorLayer = {
    label: "Hypixel error",
    color: "#e66767",
    values: errors,
  };
  const series =
    mode === "outcome"
      ? [
          errorLayer,
          { label: "First time seen", color: "#2fbf9f", values: first },
          {
            label: "Changed, snapshot stored",
            color: "#9b87f5",
            values: changed,
          },
          { label: "Unchanged", color: "#6a6d74", values: unchanged },
        ]
      : [
          errorLayer,
          {
            label: "Successful calls",
            color: "#9b87f5",
            values: calls.map((value, index) => value - errors[index]),
          },
        ];
  const switcher = el(
    "div",
    { className: "seg" },
    ...UPSTREAM_MODES.map(([key, label]) => {
      const button = el("button", {
        type: "button",
        textContent: label,
        className: key === mode ? "on" : "",
      });
      button.addEventListener("click", () => pickMode(key));
      return button;
    }),
  );
  const total = calls.reduce((sum, value) => sum + value, 0);
  const errorTotal = errors.reduce((sum, value) => sum + value, 0);
  return el(
    "section",
    { className: "panel" },
    el(
      "div",
      { className: "panel-head" },
      el("h2", { className: "panel-title", textContent: "Hypixel calls" }),
      switcher,
    ),
    legend([
      ...series,
      ...(limit
        ? [{ label: "Hypixel capacity", color: "#ffaa00", dashed: true }]
        : []),
    ]),
    stackedArea(series, limit),
    el(
      "div",
      { className: "stats" },
      stat("Hypixel Calls", FORMATS.short(total)),
      stat(
        "Hypixel Errors",
        FORMATS.number(errorTotal),
        errorTotal ? "bad" : "",
      ),
      stat(
        "Snapshots Stored",
        FORMATS.short(
          first.reduce((sum, value) => sum + value, 0) +
            changed.reduce((sum, value) => sum + value, 0),
        ),
      ),
      stat(
        "No New Snapshot",
        FORMATS.short(unchanged.reduce((sum, value) => sum + value, 0)),
      ),
      stat(
        "Capacity Now",
        data.capacity ? `${data.capacity.used} / ${data.capacity.limit}` : "—",
      ),
    ),
  );
}
