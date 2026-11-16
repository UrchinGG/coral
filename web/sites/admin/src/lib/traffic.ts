import { load } from "@/lib/api";
import { legend, stackedArea, stackedBars } from "@/lib/chart";
import { el, pressable } from "@/lib/dom";
import { filterChip, type FilterState, type Pick } from "@/lib/filterbar";
import { FORMATS } from "@/lib/format";
import { pager, type CursorPage } from "@/lib/pager";
import { refLink, type Ref } from "@/lib/refs";
import {
  methodBadge,
  requestHead,
  requestRow,
  viaBadge,
  type RequestRow,
} from "@/lib/requests";
import { ago, moment } from "@/lib/time";
import { API } from "@/lib/paths";

export const WINDOWS: [string, string, number][] = [
  ["1h", "1 h", 3_600_000],
  ["6h", "6 h", 21_600_000],
  ["24h", "24 h", 86_400_000],
  ["7d", "7 d", 604_800_000],
  ["30d", "30 d", 2_592_000_000],
];
const TRAFFIC_WINDOWS = WINDOWS.slice(2);
export const DEFAULT_WINDOW = "24h";
const UNMATCHED = "unmatched";

export interface TrafficState {
  window: string;
  routes: string[];
  routeSearch: FilterState;
  requestSearch: FilterState;
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

export interface Caller {
  requester: Ref;
  key_kind: string;
  key_prefix: string | null;
  requests: number;
  rate_limited: number;
  errors: number;
  last_hour: string;
}

interface Totals {
  requests: number;
  errors: number;
  rate_limited: { caller: number; upstream: number };
  p95: number | null;
}

interface SeriesPoint {
  t: string;
  c2xx: number;
  c4xx: number;
  c429_caller: number;
  c429_upstream: number;
  c5xx: number;
}

interface Upstream {
  series: { t: string; requests: number; errors: number }[];
  snapshots: { t: string; first: number; delta: number }[];
  capacity: { used: number; limit: number } | null;
  step_s: number;
}

interface Brush {
  pick: (pick: Pick) => void;
}

const HOUR_MS = 3_600_000;
const SHORT_TABLE_ROWS = 8;
const ERROR_RATE_WARN = 0.01;
const ERROR_RATE_BAD = 0.05;
const P95_WARN_MS = 1000;
const P95_BAD_MS = 3000;
const HYPIXEL_WINDOW_S = 300;
const TIME_SEPARATOR = "~";
const CLOCK_RANGE = /^(\d{1,2}):(\d{2})-(\d{1,2}):(\d{2})$/;
const API_GROUPS: [string, string][] = [
  ["/v3/", "Urchin v3"],
  ["/api/v1/", "Starfish"],
  ["/auth/", "Sign-in"],
];
const OTHER_API = "Other";
const REQUESTER_PARAMS: Record<string, string> = {
  member: "member",
  actor: "actor",
  key: "key_prefix",
};

const CLASSES: [keyof SeriesPoint, string, string][] = [
  ["c2xx", "Answered", "#3a9a5b"],
  ["c4xx", "Client error", "#6a6d74"],
  ["c429_caller", "Rate limited", "#e5c74a"],
  ["c429_upstream", "Hypixel limited", "#e8a23a"],
  ["c5xx", "Server error", "#e66767"],
];

const sum = (points: SeriesPoint[], keys: (keyof SeriesPoint)[]) =>
  points.reduce(
    (total, point) =>
      total + keys.reduce((part, key) => part + (point[key] as number), 0),
    0,
  );

const routeKey = (route: RouteTraffic) =>
  `${route.method} ${route.route ?? UNMATCHED}`;

const apiOf = (path: string) =>
  API_GROUPS.find(([prefix]) => path.startsWith(prefix))?.[1] ?? OTHER_API;

export const API_NAMES = [...API_GROUPS.map(([, name]) => name), OTHER_API];

const splitKey = (key: string) => {
  const [method, ...path] = key.split(" ");
  return { method, path: path.join(" ") };
};

const routeLabel = (key: string) => {
  const { method, path } = splitKey(key);
  return `${method} ${path === UNMATCHED ? "Unmatched paths" : path}`;
};

const windowSpan = (window: string) =>
  WINDOWS.find(([key]) => key === window)?.[2] ?? WINDOWS[2][2];

function routeParams(routes: string[]) {
  const parts = routes.map(splitKey);
  const methods = new Set(parts.map((part) => part.method));
  const params: Record<string, string> = {};
  if (parts.length) params.route = parts.map((part) => part.path).join(",");
  if (methods.size === 1) params.method = [...methods][0];
  return params;
}

function requesterParams(value: string | undefined) {
  if (!value) return {};
  const [kind, id] = value.split(":");
  return { [REQUESTER_PARAMS[kind] ?? "key_kind"]: id };
}

function timeBounds(state: TrafficState): Record<string, string> {
  const picked = state.requestSearch.picks.time?.value;
  if (!picked)
    return {
      from: new Date(Date.now() - windowSpan(state.window)).toISOString(),
    };
  const [from, to] = picked.split(TIME_SEPARATOR);
  return { from, to };
}

function seriesParams(state: TrafficState) {
  const picks = state.requestSearch.picks;
  const params = new URLSearchParams({
    window: state.window,
    ...(picks.via ? { key_kind: picks.via.value } : {}),
    ...requesterParams(picks.requester?.value),
  });
  const { route } = routeParams(state.routes);
  if (route) params.set("route", route);
  if (picks.class) params.set("class", picks.class.value);
  return params;
}

function requestBody(state: TrafficState) {
  const picks = state.requestSearch.picks;
  const body: Record<string, string | number> = {
    ...timeBounds(state),
    ...routeParams(state.routes),
    ...(picks.via ? { key_kind: picks.via.value } : {}),
    ...requesterParams(picks.requester?.value),
  };
  if (body.member) body.member = Number(body.member);
  if (picks.class) body.class = picks.class.value;
  if (picks.status) body.status = Number(picks.status.value);
  if (picks.ip) body.ip = picks.ip.value;
  if (state.requestSearch.q) body.q = state.requestSearch.q;
  return body;
}

export function requesterPick(caller: Caller, label: string): Pick {
  const ref = caller.requester;
  const value =
    ref.kind === "account"
      ? ref.member_id !== null
        ? `member:${ref.member_id}`
        : `actor:${ref.discord_id}`
      : caller.key_prefix
        ? `key:${caller.key_prefix}`
        : `kind:${caller.key_kind}`;
  return { value, label };
}

export function clockRange(raw: string): Pick | null {
  const match = CLOCK_RANGE.exec(raw);
  if (!match) return null;
  const [startHour, startMinute, endHour, endMinute] = match
    .slice(1)
    .map(Number);
  const from = new Date();
  from.setHours(startHour, startMinute, 0, 0);
  if (from.getTime() > Date.now()) from.setDate(from.getDate() - 1);
  const length =
    (((endHour - startHour) * 60 + endMinute - startMinute + 1440) % 1440) *
    60_000;
  if (!length) return null;
  const to = new Date(from.getTime() + length);
  return timePick(from, to, raw.replace("-", " to "));
}

const timePick = (from: Date, to: Date, label: string): Pick => ({
  value: `${from.toISOString()}${TIME_SEPARATOR}${to.toISOString()}`,
  label,
});

const stat = (label: string, value: string, className = "") =>
  el(
    "div",
    { className: "stat" },
    el("span", { className: "label", textContent: label }),
    el("b", { className, textContent: value }),
  );

export async function summary(window: string) {
  const data = await load<{ totals: Totals; callers: number }>(
    `/traffic/summary?window=${window}`,
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
    stat("Callers", FORMATS.number(data.callers)),
  );
}

export async function chart(
  state: TrafficState,
  pickWindow: (window: string) => void,
  brush: Brush,
) {
  const data = await load<{ points: SeriesPoint[]; step_s: number }>(
    `/traffic/series?${seriesParams(state)}`,
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
    el(
      "div",
      { className: "panel-head" },
      el("h2", {
        className: "panel-title",
        textContent: state.routes.length
          ? `Requests to ${state.routes.map(routeLabel).join(", ")}`
          : "Requests by status class",
      }),
      windowPicker(state.window, pickWindow),
    ),
    legend(
      CLASSES.map(([key, label, color]) => ({
        label: `${label} ${FORMATS.short(sum(data.points, [key]))}`,
        color,
      })),
    ),
    brushArea(bars, data.points, data.step_s * 1000, brush, state),
  );
}

function windowPicker(current: string, pick: (window: string) => void) {
  const seg = el(
    "div",
    { className: "seg" },
    ...TRAFFIC_WINDOWS.map(([key, label]) => {
      const button = el("button", {
        type: "button",
        textContent: label,
        className: key === current ? "on" : "",
      });
      button.setAttribute("aria-pressed", String(key === current));
      button.addEventListener("click", () => pick(key));
      return button;
    }),
  );
  seg.setAttribute("role", "group");
  seg.setAttribute("aria-label", "Time range");
  return seg;
}

function brushArea(
  bars: HTMLElement,
  points: SeriesPoint[],
  stepMs: number,
  brush: Brush,
  state: TrafficState,
) {
  const selection = el("div", { className: "brush-sel", hidden: true });
  const picked = state.requestSearch.picks.time;
  const note = el("p", {
    className: "dim brush-note",
    textContent: picked
      ? `Showing ${picked.label} in the list below.`
      : "Drag across the chart to look at one period.",
  });
  const area = el("div", { className: "brush-area" }, bars, selection);
  const at = (event: MouseEvent) => {
    const box = area.getBoundingClientRect();
    const share = (event.clientX - box.left) / box.width;
    return Math.max(
      0,
      Math.min(points.length - 1, Math.floor(share * points.length)),
    );
  };
  let start: number | null = null;
  const draw = (a: number, b: number) => {
    selection.hidden = false;
    selection.style.left = `${(Math.min(a, b) / points.length) * 100}%`;
    selection.style.width = `${((Math.abs(b - a) + 1) / points.length) * 100}%`;
  };
  area.addEventListener("mousedown", (event) => {
    start = at(event);
    draw(start, start);
  });
  area.addEventListener("mousemove", (event) => {
    if (start !== null) draw(start, at(event));
  });
  const finish = (event: MouseEvent) => {
    if (start === null || !points.length) return;
    const [a, b] = [start, at(event)].sort((x, y) => x - y);
    start = null;
    const from = new Date(points[a].t);
    const to = new Date(Date.parse(points[b].t) + stepMs);
    brush.pick(
      timePick(
        from,
        to,
        `${moment(points[a].t)} to ${moment(to.toISOString())}`,
      ),
    );
  };
  const shown = picked && selectedBars(points, stepMs, picked.value);
  if (shown) draw(...shown);
  area.addEventListener("mouseup", finish);
  area.addEventListener(
    "mouseleave",
    (event) => start !== null && finish(event),
  );
  return el("div", {}, area, note);
}

function selectedBars(points: SeriesPoint[], stepMs: number, value: string) {
  const [from, to] = value.split(TIME_SEPARATOR).map(Date.parse);
  const first = points.findIndex(
    (point) => Date.parse(point.t) + stepMs > from,
  );
  const last = points.findLastIndex((point) => Date.parse(point.t) < to);
  return first >= 0 && last >= first ? ([first, last] as const) : null;
}

export async function loadRoutes(window: string) {
  return load<RouteTraffic[]>(`/traffic/routes?window=${window}`);
}

export function routeTable(
  routes: RouteTraffic[],
  state: TrafficState,
  toggle: (key: string) => void,
) {
  const { q, picks } = state.routeSearch;
  const text = q.toLowerCase();
  const shown = routes.filter((route) => {
    const label = routeLabel(routeKey(route)).toLowerCase();
    return (
      (!text || label.includes(text)) &&
      (!picks.route || label.includes(picks.route.value.toLowerCase())) &&
      (!picks.method || route.method === picks.method.value) &&
      (!picks.api || apiOf(route.route ?? "") === picks.api.value)
    );
  });
  const head = el(
    "div",
    { className: "table-row route-row head" },
    ...["Route", "Requests", "Errors", "429", "P95"].map((label, index) =>
      el("span", { className: index ? "num" : "", textContent: label }),
    ),
  );
  const rows = shown.map((route) => {
    const key = routeKey(route);
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
        className: `num ${errorRateClass(route.error_rate)}`,
        textContent: `${(route.error_rate * 100).toFixed(1)}%`,
      }),
      count(route.rate_limited, "warn"),
      el("span", {
        className: `num ${p95Class(route.p95)}`,
        textContent: route.p95 === null ? "" : `${route.p95} ms`,
      }),
    );
    line.setAttribute("aria-pressed", String(state.routes.includes(key)));
    line.addEventListener("click", () => toggle(key));
    return line;
  });
  const empty = el("p", {
    className: "muted table-empty",
    textContent: "No route matches.",
  });
  return rows.length
    ? shortTable(head, rows)
    : el("div", { className: "table" }, head, empty);
}

const errorRateClass = (rate: number) =>
  rate >= ERROR_RATE_BAD ? "bad" : rate >= ERROR_RATE_WARN ? "warn" : "dim";

const p95Class = (p95: number | null) =>
  p95 === null || p95 < P95_WARN_MS ? "" : p95 < P95_BAD_MS ? "warn" : "bad";

const count = (value: number, className: string) =>
  el("span", {
    className: `num ${value ? className : "dim"}`,
    textContent: FORMATS.number(value),
  });

function shortTable(head: HTMLElement, rows: HTMLElement[]) {
  const table = el("div", { className: "table" }, head, ...rows);
  if (rows.length <= SHORT_TABLE_ROWS) return table;
  const hidden = rows.slice(SHORT_TABLE_ROWS);
  for (const row of hidden) row.hidden = true;
  const more = el("button", {
    type: "button",
    className: "table-more",
    textContent: `Show all ${rows.length}`,
  });
  more.addEventListener("click", () => {
    for (const row of hidden) row.hidden = false;
    more.remove();
  });
  table.append(more);
  return table;
}

export function routePicker(
  routes: RouteTraffic[],
  chosen: string[],
  set: (routes: string[]) => void,
  open: boolean,
  setOpen: (open: boolean) => void,
) {
  const menu = el("div", { className: "pick-menu", hidden: !open });
  menu.setAttribute("role", "group");
  menu.setAttribute("aria-label", "Routes to show");
  const keys = [...new Set([...routes.map(routeKey), ...chosen])];
  for (const api of API_NAMES) {
    const names = keys.filter((key) => apiOf(splitKey(key).path) === api);
    if (!names.length) continue;
    const all = names.every((name) => chosen.includes(name));
    menu.append(
      pickButton("pick-group", all, [el("span", { textContent: api })], () =>
        set(
          all
            ? chosen.filter((key) => !names.includes(key))
            : [...new Set([...chosen, ...names])],
        ),
      ),
      ...names.map((name) => {
        const { method, path } = splitKey(name);
        const on = chosen.includes(name);
        return pickButton(
          "pick-item",
          on,
          [
            methodBadge(method),
            el("span", {
              textContent: path === UNMATCHED ? "Unmatched paths" : path,
            }),
          ],
          () =>
            set(on ? chosen.filter((key) => key !== name) : [...chosen, name]),
        );
      }),
    );
  }
  const summaryText = !chosen.length
    ? "All routes"
    : chosen.length === 1
      ? routeLabel(chosen[0])
      : `${chosen.length} routes`;
  const toggle = el("button", {
    type: "button",
    className: "button small",
    textContent: `Routes: ${summaryText} ▾`,
  });
  toggle.setAttribute("aria-expanded", String(open));
  toggle.addEventListener("click", () => setOpen(!open));
  const chips = chosen.map((key) =>
    filterChip(
      `Remove ${routeLabel(key)}`,
      () => set(chosen.filter((other) => other !== key)),
      routeLabel(key),
    ),
  );
  const clear = el("button", {
    type: "button",
    className: "button small",
    textContent: "All routes",
    hidden: !chosen.length,
  });
  clear.addEventListener("click", () => set([]));
  return el("div", { className: "picker" }, toggle, ...chips, clear, menu);
}

function pickButton(
  className: string,
  on: boolean,
  content: Node[],
  toggle: () => void,
) {
  const button = el(
    "button",
    { type: "button", className: on ? `${className} on` : className },
    el("span", { className: "box" }),
    ...content,
  );
  button.setAttribute("aria-pressed", String(on));
  button.addEventListener("click", toggle);
  return button;
}

export async function loadCallers(state: TrafficState) {
  const params = new URLSearchParams({
    window: state.window,
    ...routeParams(state.routes),
  });
  const picked = state.requestSearch.picks.class;
  if (picked) params.set("class", picked.value);
  return (await load<{ callers: Caller[] }>(`/traffic/callers?${params}`))
    .callers;
}

const lastSeen = (hour: string) =>
  Date.now() - Date.parse(hour) < HOUR_MS
    ? "This hour"
    : ago(new Date(Date.parse(hour) + HOUR_MS).toISOString());

export function callerTable(
  callers: Caller[],
  picked: string | undefined,
  pick: (caller: Caller, name: HTMLElement) => void,
) {
  const head = el(
    "div",
    { className: "table-row caller head" },
    ...["Requester", "Via", "Requests", "429", "Errors", "Last"].map(
      (label, index) =>
        el("span", {
          className: index > 1 && index < 5 ? "num" : "",
          textContent: label,
        }),
    ),
  );
  const empty = { discord: {}, minecraft: {} };
  const rows = callers.map((caller) => {
    const name = refLink(caller.requester, empty);
    name.dataset.requester = requesterPick(caller, "").value;
    const line = el(
      "div",
      { className: "table-row caller" },
      el("span", {}, name),
      el("span", {}, viaBadge(caller.key_kind)),
      el("span", {
        className: "num",
        textContent: FORMATS.number(caller.requests),
      }),
      count(caller.rate_limited, "warn"),
      count(caller.errors, "bad"),
      el("span", { className: "dim", textContent: lastSeen(caller.last_hour) }),
    );
    line.setAttribute(
      "aria-pressed",
      String(name.dataset.requester === picked),
    );
    pressable(line, () => pick(caller, name));
    return line;
  });
  return shortTable(head, rows);
}

export function requests(state: TrafficState) {
  const body = requestBody(state);
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
