import { load } from "@/lib/api";
import { el, panel } from "@/lib/dom";
import {
  filterBar,
  rangeStart,
  type FilterState,
  type Pick,
} from "@/lib/filterbar";
import { FORMATS } from "@/lib/format";
import { keySeries, switchableChart, type KeySeries } from "@/lib/keychart";
import type { Labels } from "@/lib/members";
import { pager } from "@/lib/pager";
import { refLink, type Ref } from "@/lib/refs";
import { requestHead, requestRow, type RequestRow } from "@/lib/requests";
import { moment } from "@/lib/time";

interface Activity {
  totals: {
    requests: number;
    rate_limited_caller: number;
    rate_limited_upstream: number;
    errors: number;
  };
  mentions: number;
  series: {
    t: string;
    requests: number;
    rate_limited_caller: number;
    rate_limited_upstream: number;
    errors: number;
  }[];
  by_key: KeySeries[];
  breakdown: {
    route: string | null;
    method: string;
    status: number;
    requests: number;
  }[];
}

interface TopLookups {
  players: { player: Ref; requests: number; last: string; via: string }[];
  labels: Labels;
}

const WINDOWS: [string, string, string, string][] = [
  ["24h", "24 hours", "last 24 hours", "24h"],
  ["7d", "7 days", "last 7 days", "7d"],
  ["30d", "30 days", "last 30 days", "30d"],
  ["400d", "All", "all retained", "all"],
];
const STATUS_CLASSES: Pick[] = [
  { value: "2xx", label: "2xx" },
  { value: "4xx", label: "4xx" },
  { value: "rate_limited", label: "Rate limited" },
  { value: "5xx", label: "5xx" },
];
const VIAS: Pick[] = [
  { value: "personal", label: "Personal key" },
  { value: "developer", label: "Developer key" },
  { value: "internal", label: "Bot" },
  { value: "session", label: "Session" },
  { value: "minecraft", label: "Minecraft" },
];
const TOP_WINDOW = "14d";
const TOP_SHOWN = 10;
const ROUTES_SHOWN = 10;
const RATE_LIMITED = 429;
const FIRST_SERVER_ERROR = 500;
const COLORS = {
  answered: "#3a9a5b",
  limited: "#e5c74a",
  errors: "#e66767",
};

export function memberRequestsTab(id: string) {
  const activity = el("div", { className: "stack" });
  const windows = el("div", { className: "seg" });
  const stream = requestStream(id);
  const pick = (window: string) => {
    windows.querySelectorAll("button").forEach((button) => {
      button.className = button.dataset.window === window ? "on" : "";
    });
    activity.replaceChildren(el("p", { className: "muted" }, "Loading…"));
    stream.range(WINDOWS.find(([key]) => key === window)?.[3] ?? "24h");
    showActivity(id, window, activity, stream.routes).catch((error: Error) =>
      activity.replaceChildren(el("p", { className: "bad" }, error.message)),
    );
  };
  for (const [key, label] of WINDOWS) {
    const button = el("button", { type: "button", textContent: label });
    button.dataset.window = key;
    button.addEventListener("click", () => pick(key));
    windows.append(button);
  }
  pick(WINDOWS[0][0]);
  return el("div", { className: "stack" }, windows, activity, stream.element);
}

export async function activityChart(id: string) {
  const [key] = WINDOWS[0];
  const activity = await load<Activity>(
    `/accounts/${id}/activity?window=${key}`,
  );
  return chartPanel(activity, key);
}

function requestStream(id: string) {
  const routes = new Set<string>();
  const ips = new Set<string>();
  const state: FilterState = { q: "", picks: {}, range: "24h" };
  const table = el("div", { className: "table" });
  const show = () => {
    const query = new URLSearchParams(
      Object.entries(state.picks).map(([key, pick]) => [key, pick.value]),
    );
    const from = rangeStart(state.range);
    if (from) query.set("from", from);
    if (state.q) query.set("q", state.q);
    table.replaceChildren(
      requestHead(),
      pager<RequestRow>(
        `/accounts/${id}/requests?${query}`,
        (row, labels) => {
          if (row.ip?.kind === "ip") ips.add(row.ip.ip);
          return requestRow(row, labels);
        },
        "No requests match.",
      ),
    );
  };
  const bar = filterBar({
    placeholder: "Search paths, errors and user agents",
    state,
    fields: [
      { key: "route", label: "Route", values: () => [...routes].map(same) },
      { key: "class", label: "Status", values: () => STATUS_CLASSES },
      { key: "ip", label: "IP", values: () => [...ips].map(same) },
      { key: "key_kind", label: "Via", values: () => VIAS },
    ],
    onChange: show,
  });
  return {
    element: el("div", { className: "list" }, bar.element, table),
    routes,
    range: (picked: string) => {
      state.range = picked;
      show();
    },
  };
}

const same = (value: string): Pick => ({ value, label: value });

async function showActivity(
  id: string,
  window: string,
  into: HTMLElement,
  routesSeen: Set<string>,
) {
  const [activity, top] = await Promise.all([
    load<Activity>(`/accounts/${id}/activity?window=${window}`),
    load<TopLookups>(`/accounts/${id}/lookups/top?window=${TOP_WINDOW}`),
  ]);
  for (const entry of activity.breakdown)
    if (entry.route) routesSeen.add(entry.route);
  into.replaceChildren(
    chartPanel(activity, window),
    el(
      "div",
      { className: "panels" },
      topPlayers(top),
      routes(activity, window),
    ),
  );
}

function chartPanel(activity: Activity, window: string) {
  const limited = activity.series.map(
    (point) => point.rate_limited_caller + point.rate_limited_upstream,
  );
  const errors = activity.series.map((point) => point.errors);
  const series = [
    {
      label: "Answered",
      color: COLORS.answered,
      values: activity.series.map((point, index) =>
        Math.max(0, point.requests - limited[index] - errors[index]),
      ),
    },
    { label: "Rate limited", color: COLORS.limited, values: limited },
    { label: "Errors", color: COLORS.errors, values: errors },
  ];
  const { totals } = activity;
  const label = WINDOWS.find(([key]) => key === window)?.[2] ?? window;
  const chart = switchableChart(
    [
      { label: "By key", series: keySeries(activity.by_key) },
      { label: "By status", series },
    ],
    activity.series.map((point) => moment(point.t)),
  );
  return el(
    "section",
    { className: "panel wide" },
    el(
      "div",
      { className: "panel-head" },
      el("h2", { className: "panel-title" }, `Requests, ${label}`),
      chart.switcher,
    ),
    chart.body,
    el(
      "div",
      { className: "stats" },
      ...[
        ["Requests", totals.requests],
        [
          "Rate Limited",
          totals.rate_limited_caller + totals.rate_limited_upstream,
        ],
        ["Errors", totals.errors],
        ["Players Looked Up", activity.mentions],
      ].map(([name, value]) =>
        el(
          "div",
          { className: "stat" },
          el("span", { className: "label" }, String(name)),
          el("b", {}, FORMATS.number(Number(value))),
        ),
      ),
    ),
  );
}

function topPlayers(top: TopLookups) {
  const players = top.players.slice(0, TOP_SHOWN);
  return panel(
    `Top players looked up · ${TOP_WINDOW}`,
    ...(players.length
      ? players.map((entry) =>
          line(
            refLink(entry.player, top.labels),
            el("span", { className: "dim" }, FORMATS.number(entry.requests)),
          ),
        )
      : [el("p", { className: "muted" }, "No players looked up.")]),
  );
}

function routes(activity: Activity, window: string) {
  const rows = [...activity.breakdown]
    .sort((a, b) => b.requests - a.requests)
    .slice(0, ROUTES_SHOWN);
  const label = WINDOWS.find(([key]) => key === window)?.[1] ?? window;
  return panel(
    `Routes · ${label}`,
    ...(rows.length
      ? rows.map((entry) =>
          line(
            el(
              "span",
              { className: "inline" },
              el(
                "span",
                { className: "mono" },
                `${entry.method} ${entry.route ?? "unmatched"}`,
              ),
              el(
                "span",
                { className: statusClass(entry.status) },
                String(entry.status),
              ),
            ),
            el("span", { className: "dim" }, FORMATS.number(entry.requests)),
          ),
        )
      : [el("p", { className: "muted" }, "No requests in this window.")]),
  );
}

function line(name: Node, count: Node) {
  return el("div", { className: "kv-row" }, name, count);
}

function statusClass(status: number) {
  if (status >= FIRST_SERVER_ERROR) return "bad";
  return status === RATE_LIMITED ? "warn" : "ok";
}
