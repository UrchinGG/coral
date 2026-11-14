import { el } from "@/lib/dom";

const SVG = "http://www.w3.org/2000/svg";

export interface Series {
  label: string;
  color: string;
  values: number[];
  fill?: string;
}

export function legend(
  series: { label: string; color: string; dashed?: boolean }[],
) {
  return el(
    "div",
    { className: "legend" },
    ...series.map((entry) => {
      const swatch = el("i", {
        className: entry.dashed ? "swatch dashed" : "swatch",
      });
      swatch.style.background = entry.dashed ? "transparent" : entry.color;
      swatch.style.borderColor = entry.color;
      return el("span", {}, swatch, entry.label);
    }),
  );
}

export function stackedBars(series: Series[], titles: string[]) {
  const count = Math.max(0, ...series.map((entry) => entry.values.length));
  const totals = Array.from({ length: count }, (_, index) =>
    series.reduce((sum, entry) => sum + (entry.values[index] ?? 0), 0),
  );
  const peak = Math.max(1, ...totals);
  return el(
    "div",
    { className: "stack-bars" },
    ...totals.map((total, index) => {
      const column = el("span", {
        className: "column",
        title: `${titles[index] ?? ""} · ${total}`,
      });
      column.style.height = `${(total / peak) * 100}%`;
      for (const entry of series) {
        const value = entry.values[index] ?? 0;
        if (!value) continue;
        const piece = el("i");
        piece.style.flexGrow = String(value);
        piece.style.background = entry.color;
        column.append(piece);
      }
      return column;
    }),
  );
}

function path(
  values: number[],
  lower: number[],
  peak: number,
  width: number,
  height: number,
) {
  const x = (index: number) =>
    ((index / Math.max(1, values.length - 1)) * width).toFixed(1);
  const y = (value: number) => (height - (value / peak) * height).toFixed(1);
  const top = values
    .map((value, index) => `${index ? "L" : "M"}${x(index)} ${y(value)}`)
    .join(" ");
  const bottom = lower
    .map((value, index) => `L${x(index)} ${y(value)}`)
    .reverse()
    .join(" ");
  return { area: `${top} ${bottom} Z`, line: top };
}

export function stackedArea(
  series: Series[],
  limit: number | null,
  height = 160,
) {
  const width = 600;
  const tops = series.reduce<number[][]>(
    (acc, entry) =>
      acc.concat([
        entry.values.map((value, index) => value + (acc.at(-1)?.[index] ?? 0)),
      ]),
    [],
  );
  const peak = Math.max(1, limit ?? 0, ...(tops.at(-1) ?? [])) * 1.1;
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("class", "area-chart");
  svg.style.height = `${height}px`;
  tops.forEach((top, index) => {
    const lower = index ? tops[index - 1] : top.map(() => 0);
    const { area, line } = path(top, lower, peak, width, height);
    const fill = document.createElementNS(SVG, "path");
    fill.setAttribute("d", area);
    fill.setAttribute("fill", series[index].fill ?? series[index].color);
    fill.setAttribute("fill-opacity", series[index].fill ? "1" : "0.45");
    const stroke = document.createElementNS(SVG, "path");
    stroke.setAttribute("d", line);
    stroke.setAttribute("fill", "none");
    stroke.setAttribute("stroke", series[index].color);
    stroke.setAttribute("stroke-width", "1.5");
    stroke.setAttribute("vector-effect", "non-scaling-stroke");
    svg.append(fill, stroke);
  });
  if (limit) {
    const y = (height - (limit / peak) * height).toFixed(1);
    const line = document.createElementNS(SVG, "line");
    for (const [name, value] of [
      ["x1", "0"],
      ["x2", String(width)],
      ["y1", y],
      ["y2", y],
      ["stroke", "#ffaa00"],
      ["stroke-width", "1.5"],
      ["stroke-dasharray", "6 5"],
      ["vector-effect", "non-scaling-stroke"],
    ])
      line.setAttribute(name, value);
    svg.append(line);
  }
  return svg;
}
