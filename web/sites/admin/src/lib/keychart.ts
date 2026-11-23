import { legend, stackedArea, type Series } from "@/lib/chart";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";

export interface KeySeries {
  key_kind: string;
  values: number[];
}

export interface KeyActivity {
  from: string;
  step_s: number;
  by_key: KeySeries[];
}

type ActivityLoader = (window: string) => Promise<KeyActivity | undefined>;

const WINDOWS: [string, string][] = [
  ["24h", "24 h"],
  ["7d", "7 d"],
  ["30d", "30 d"],
];
const SOURCES: Record<string, { label: string; color: string }> = {
  personal: { label: "API key", color: "#5b8def" },
  session: { label: "Starfish", color: "#3fb8af" },
  minecraft: { label: "Minecraft", color: "#55c06a" },
  internal: { label: "Urchin bot", color: "#a78bfa" },
  developer: { label: "Developer key", color: "#e8a23a" },
};
const CHART_HEIGHT = 120;

export function sharedActivity(load: ActivityLoader): ActivityLoader {
  const loaded = new Map<string, Promise<KeyActivity | undefined>>();
  return (window) => {
    if (!loaded.has(window)) loaded.set(window, load(window));
    return loaded.get(window)!;
  };
}

/** Requests made with one key. */
export function keyChart(kind: string, load: ActivityLoader) {
  return activityPanel(load, (activity) => [seriesFor(activity, kind)], false);
}

/** Every request a person made without their developer key, stacked by source. */
export function activityChart(load: ActivityLoader) {
  return activityPanel(
    load,
    (activity) =>
      Object.keys(SOURCES)
        .filter((kind) => kind !== "developer")
        .filter((kind) => activity.by_key.some((key) => key.key_kind === kind))
        .map((kind) => seriesFor(activity, kind)),
    true,
  );
}

function activityPanel(
  load: ActivityLoader,
  seriesOf: (activity: KeyActivity) => Series[],
  withLegend: boolean,
) {
  const total = el("b", { className: "key-chart-total" });
  const seg = el("div", { className: "seg" });
  seg.setAttribute("role", "group");
  seg.setAttribute("aria-label", "Time range");
  const plot = el("div");
  const show = async (window: string) => {
    seg.replaceChildren(
      ...WINDOWS.map(([key, label]) =>
        windowButton(key, label, key === window, show),
      ),
    );
    const activity = await load(window);
    if (!activity) {
      plot.replaceChildren(
        el("p", {
          className: "muted",
          textContent: "Requests are unavailable.",
        }),
      );
      return;
    }
    const series = seriesOf(activity);
    const sum = series
      .flatMap((entry) => entry.values)
      .reduce((acc, value) => acc + value, 0);
    const label = WINDOWS.find(([key]) => key === window)?.[1] ?? window;
    total.textContent = `${FORMATS.number(sum)} requests, ${label}`;
    plot.replaceChildren(
      ...(withLegend && series.length ? [legend(series)] : []),
      stackedArea(series, null, CHART_HEIGHT),
    );
  };
  show(WINDOWS[0][0]);
  return el(
    "div",
    { className: "key-chart" },
    el("div", { className: "key-chart-head" }, total, seg),
    plot,
  );
}

function seriesFor(activity: KeyActivity, kind: string): Series {
  const steps = Math.max(0, ...activity.by_key.map((key) => key.values.length));
  const source = SOURCES[kind] ?? { label: kind, color: "#8a8d94" };
  return {
    ...source,
    values:
      activity.by_key.find((key) => key.key_kind === kind)?.values ??
      new Array<number>(steps).fill(0),
  };
}

function windowButton(
  key: string,
  label: string,
  on: boolean,
  pick: (key: string) => void,
) {
  const button = el("button", {
    type: "button",
    textContent: label,
    className: on ? "on" : "",
  });
  button.setAttribute("aria-pressed", String(on));
  button.addEventListener("click", () => pick(key));
  return button;
}
