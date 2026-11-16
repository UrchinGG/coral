import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";
import type { Labels } from "@/lib/members";
import { pager, type CursorPage } from "@/lib/pager";
import { day, time } from "@/lib/time";

export interface MonthCount {
  month: string;
  count: number;
}

export interface SnapshotEntry {
  ts: string;
  source: string | null;
  baseline: boolean;
}

export interface SnapshotView {
  data: unknown;
  delta: unknown | null;
}

export type Figure = [string, string | Node, string | Node];

interface ChartOptions {
  title: string;
  count?: string;
  unit: string;
  months: MonthCount[];
  picked?: string | null;
  pick: (month: string) => void;
  action?: [string, () => void];
  figures?: Figure[];
}

interface ListOptions {
  title: string;
  pages: (cursor: string | null) => Promise<{
    rows: SnapshotEntry[];
    next_cursor: string | null;
  }>;
  view: (entry: SnapshotEntry) => Promise<SnapshotView>;
}

type Mode = "reconstruction" | "delta";

const MONTH_NAMES = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];
const MONTH_KEY_LENGTH = 7;
const MIN_BAR_PERCENT = 4;
const REDRAW = "redraw";
const NO_LABELS: Labels = { discord: {}, minecraft: {} };
const MODES: [Mode, string][] = [
  ["reconstruction", "Reconstruction"],
  ["delta", "Delta"],
];

export const monthOf = (ts: string) => ts.slice(0, MONTH_KEY_LENGTH);

export function monthLabel(month: string) {
  const [year, index] = month.split("-").map(Number);
  return `${MONTH_NAMES[index - 1]} ${year}`;
}

export function countMonths(timestamps: string[]): MonthCount[] {
  const counts = new Map<string, number>();
  for (const ts of timestamps)
    counts.set(monthOf(ts), (counts.get(monthOf(ts)) ?? 0) + 1);
  return [...counts]
    .map(([month, count]) => ({ month, count }))
    .sort((a, b) => a.month.localeCompare(b.month));
}

export function busiest(months: MonthCount[]) {
  return months.reduce<MonthCount | null>(
    (best, entry) => (!best || entry.count > best.count ? entry : best),
    null,
  );
}

export function monthChart(options: ChartOptions) {
  const months = everyMonth(options.months);
  const peak = Math.max(1, ...months.map((entry) => entry.count));
  const bars = months.map((entry) => monthBar(entry, peak, options));
  const head = el(
    "div",
    { className: "panel-head" },
    el(
      "h2",
      { className: "panel-title" },
      options.title,
      el("span", { className: "note", textContent: options.count ?? "" }),
    ),
    ...(options.action ? [actionButton(...options.action)] : []),
  );
  const chart = el(
    "section",
    { className: "panel wide month-chart" },
    head,
    el("span", { className: "muted", textContent: options.unit }),
    el("div", { className: "month-bars" }, ...bars),
    el(
      "div",
      { className: "axis" },
      el("span", {
        textContent: months.length ? monthLabel(months[0].month) : "",
      }),
      el("span", {
        textContent: months.length > 1 ? monthLabel(months.at(-1)!.month) : "",
      }),
    ),
    ...(options.figures ? [figures(options.figures)] : []),
  );
  chart.style.setProperty("--months", String(months.length));
  return chart;
}

export function figures(entries: Figure[]) {
  return el(
    "div",
    { className: "stats" },
    ...entries.map(([label, value, note]) =>
      el(
        "div",
        { className: "stat" },
        el("span", { className: "label", textContent: label }),
        el("b", {}, value),
        el("span", { className: "muted" }, note),
      ),
    ),
  );
}

export function snapshotList(options: ListOptions) {
  let mode: Mode = "reconstruction";
  const sources = new Set<string>();
  const search = el("input", {
    className: "field grow",
    type: "search",
    placeholder: "Search dates, like nov 13 or 2025",
    ariaLabel: "Search snapshots",
  });
  const source = el(
    "select",
    { className: "field", ariaLabel: "Source" },
    el("option", { value: "", textContent: "Every source" }),
  );
  const filter = () =>
    list
      .querySelectorAll<HTMLElement>(".snapshot-item")
      .forEach((item) => (item.hidden = !shown(item)));
  const shown = (item: HTMLElement) =>
    item.dataset.search!.includes(search.value.trim().toLowerCase()) &&
    (!source.value || item.dataset.source === source.value);
  const render = (entry: SnapshotEntry) => {
    if (entry.source && !sources.has(entry.source)) {
      sources.add(entry.source);
      source.append(
        el("option", { value: entry.source, textContent: entry.source }),
      );
    }
    const item = snapshotItem(entry, options.view, () => mode);
    item.hidden = !shown(item);
    return item;
  };
  const modes = modePicker(mode, (picked) => {
    mode = picked;
    list
      .querySelectorAll<HTMLElement>(".snapshot-item")
      .forEach((item) => item.dispatchEvent(new Event(REDRAW)));
  });
  search.addEventListener("input", filter);
  source.addEventListener("change", filter);
  const list = pager<SnapshotEntry>(
    (cursor) => options.pages(cursor).then(asCursorPage),
    render,
    "No snapshots.",
  );
  return el(
    "section",
    { className: "stack" },
    el("h2", { className: "group-title", textContent: options.title }),
    el("div", { className: "toolbar" }, search, source, modes),
    el(
      "div",
      { className: "table" },
      el(
        "div",
        { className: "table-row snapshot head" },
        el("span", { textContent: "When" }),
        el("span", { textContent: "Source" }),
      ),
      list,
    ),
  );
}

function modePicker(current: Mode, pick: (mode: Mode) => void) {
  const modes = el("div", { className: "seg" });
  for (const [key, label] of MODES) {
    const button = el("button", {
      type: "button",
      className: key === current ? "on" : "",
      textContent: label,
    });
    button.addEventListener("click", () => {
      modes
        .querySelectorAll("button")
        .forEach((other) => (other.className = other === button ? "on" : ""));
      pick(key);
    });
    modes.append(button);
  }
  return modes;
}

function asCursorPage(page: {
  rows: SnapshotEntry[];
  next_cursor: string | null;
}): CursorPage<SnapshotEntry> {
  return { items: page.rows, next_cursor: page.next_cursor, labels: NO_LABELS };
}

function snapshotItem(
  entry: SnapshotEntry,
  view: ListOptions["view"],
  mode: () => Mode,
) {
  const when = `${day(entry.ts)}, ${time(entry.ts)}`;
  const origin = [entry.source ?? "", entry.baseline ? "baseline" : ""]
    .filter(Boolean)
    .join(", ");
  const detail = el("div", { className: "row-detail", hidden: true });
  const line = el(
    "button",
    { type: "button", className: "table-row snapshot" },
    el("span", { textContent: when }),
    el("span", { className: "dim", textContent: origin }),
  );
  const item = el("div", { className: "snapshot-item" }, line, detail);
  item.dataset.search = `${when} ${origin}`.toLowerCase();
  item.dataset.source = entry.source ?? "";
  let loaded: Promise<SnapshotView> | undefined;
  const draw = async () => {
    if (detail.hidden) return;
    detail.replaceChildren(
      el("p", { className: "muted", textContent: "Loading…" }),
    );
    try {
      loaded ??= view(entry);
      detail.replaceChildren(...snapshotBody(await loaded, mode()));
    } catch (error) {
      loaded = undefined;
      detail.replaceChildren(
        el("p", { className: "muted", textContent: (error as Error).message }),
      );
    }
  };
  line.addEventListener("click", () => {
    detail.hidden = !detail.hidden;
    line.setAttribute("aria-expanded", String(!detail.hidden));
    draw();
  });
  item.addEventListener(REDRAW, draw);
  return item;
}

function snapshotBody(snapshot: SnapshotView, mode: Mode) {
  const stored = snapshot.delta === null;
  const delta = mode === "delta" && !stored;
  const note = stored
    ? "Baseline snapshot, stored in full."
    : delta
      ? "Only what changed since the snapshot before."
      : "Rebuilt from the baseline and every change up to this snapshot.";
  return [
    el("p", { className: "muted", textContent: note }),
    el("pre", {
      className: "code",
      textContent: JSON.stringify(
        delta ? snapshot.delta : snapshot.data,
        null,
        2,
      ),
    }),
  ];
}

function everyMonth(months: MonthCount[]): MonthCount[] {
  if (!months.length) return [];
  const counts = new Map(months.map((entry) => [entry.month, entry.count]));
  const [year, index] = months[0].month.split("-").map(Number);
  const cursor = new Date(Date.UTC(year, index - 1, 1));
  const last = monthOf(new Date().toISOString());
  const filled: MonthCount[] = [];
  for (
    let key = monthOf(cursor.toISOString());
    key <= last;
    key = monthOf(cursor.toISOString())
  ) {
    filled.push({ month: key, count: counts.get(key) ?? 0 });
    cursor.setUTCMonth(cursor.getUTCMonth() + 1);
  }
  return filled;
}

function monthBar(entry: MonthCount, peak: number, options: ChartOptions) {
  const label = `${monthLabel(entry.month)}, ${FORMATS.number(entry.count)} ${entry.count === 1 ? "snapshot" : "snapshots"}`;
  const fill = el("span");
  fill.style.height = entry.count
    ? `${Math.max(MIN_BAR_PERCENT, (entry.count / peak) * 100)}%`
    : "";
  const bar = el(
    "button",
    {
      type: "button",
      className: entry.count ? "month-bar" : "month-bar empty",
      title: label,
      ariaLabel: label,
    },
    fill,
  );
  bar.setAttribute("aria-pressed", String(entry.month === options.picked));
  bar.addEventListener("click", () => options.pick(entry.month));
  return bar;
}

function actionButton(label: string, run: () => void) {
  const button = el("button", {
    type: "button",
    className: "button small",
    textContent: label,
  });
  button.addEventListener("click", run);
  return button;
}
