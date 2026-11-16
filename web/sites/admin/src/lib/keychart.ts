import { legend, stackedBars, type Series } from "@/lib/chart";
import { el } from "@/lib/dom";
import { FORMATS } from "@/lib/format";

export interface KeySeries {
  key_kind: string;
  values: number[];
}

export interface ChartView {
  label: string;
  series: Series[];
}

const KEYS: Record<string, [string, string]> = {
  personal: ["Personal key", "#5b8def"],
  developer: ["Developer key", "#e8a23a"],
  session: ["Starfish", "#2fbf9f"],
  minecraft: ["Minecraft", "#b7a7f7"],
  internal: ["Bot", "#8a8d94"],
};
const OTHER_KEY: [string, string] = ["Other", "#5a5d64"];

export function keySeries(byKey: KeySeries[]): Series[] {
  return byKey.map((entry) => {
    const [label, color] = KEYS[entry.key_kind] ?? OTHER_KEY;
    const total = entry.values.reduce((sum, value) => sum + value, 0);
    return {
      label: `${label} ${FORMATS.short(total)}`,
      color,
      values: entry.values,
    };
  });
}

export function switchableChart(views: ChartView[], titles: string[]) {
  const body = el("div");
  const buttons = views.map((view) => {
    const button = el("button", { type: "button", textContent: view.label });
    button.addEventListener("click", () => show(view));
    return button;
  });
  const show = (view: ChartView) => {
    buttons.forEach((button, index) => {
      const on = views[index] === view;
      button.className = on ? "on" : "";
      button.setAttribute("aria-pressed", String(on));
    });
    body.replaceChildren(
      legend(view.series),
      view.series.length
        ? stackedBars(view.series, titles)
        : el("p", { className: "muted", textContent: "No requests." }),
    );
  };
  const switcher = el("div", { className: "seg" }, ...buttons);
  switcher.setAttribute("role", "group");
  switcher.setAttribute("aria-label", "Split requests by");
  show(views[0]);
  return { switcher, body };
}
