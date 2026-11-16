import { stackedBars } from "@/lib/chart";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";
import { moment } from "@/lib/time";

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
const KEY_COLORS: Record<string, string> = {
  personal: "#5b8def",
  developer: "#e8a23a",
};
const FALLBACK_COLOR = "#8a8d94";

export function sharedActivity(load: ActivityLoader): ActivityLoader {
  const loaded = new Map<string, Promise<KeyActivity | undefined>>();
  return (window) => {
    if (!loaded.has(window)) loaded.set(window, load(window));
    return loaded.get(window)!;
  };
}

export function keyChart(kind: string, load: ActivityLoader) {
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
    const values = valuesFor(activity, kind);
    const label = WINDOWS.find(([key]) => key === window)?.[1] ?? window;
    total.textContent = `${FORMATS.number(values.reduce((sum, value) => sum + value, 0))} requests, ${label}`;
    plot.replaceChildren(
      stackedBars(
        [{ label: kind, color: KEY_COLORS[kind] ?? FALLBACK_COLOR, values }],
        values.map((_, index) => moment(stepStart(activity, index))),
      ),
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

function valuesFor(activity: KeyActivity, kind: string) {
  const steps = Math.max(0, ...activity.by_key.map((key) => key.values.length));
  return (
    activity.by_key.find((key) => key.key_kind === kind)?.values ??
    new Array<number>(steps).fill(0)
  );
}

function stepStart(activity: KeyActivity, index: number) {
  return new Date(
    Date.parse(activity.from) + index * activity.step_s * 1000,
  ).toISOString();
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
