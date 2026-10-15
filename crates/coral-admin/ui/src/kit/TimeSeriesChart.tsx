import { useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";

import { formatNumber, plural } from "../lib/format";
import { parseTimestamp } from "../lib/time";
import { EmptyState } from "./States";

export type ChartSeries = { key: string; label: string; color: string; opacity?: number };
export type ChartPoint = { t: string; values: Record<string, number>; line?: number | null };
export type ChartLine = { label: string; unit: string; color: string };
export type ChartRange = { from: string; to: string };

type TimeSeriesChartProps = {
  label: string;
  points: readonly ChartPoint[];
  stepMs: number;
  series: readonly ChartSeries[];
  unit: string;
  line?: ChartLine;
  brush?: ChartRange | null;
  onBrush?: (range: ChartRange | null) => void;
  onPointClick?: (t: string) => void;
  formatTick: (t: string) => string;
  formatTooltipTime: (t: string) => string;
};

type ChartPointerInput = Pick<TimeSeriesChartProps, "points" | "stepMs" | "onBrush" | "onPointClick"> & {
  brush: ChartRange | null;
  geometry: Geometry;
};

type SvgPointerEvent = ReactPointerEvent<SVGSVGElement>;
type Fractions = readonly [number, number];
type DragMode = "body" | "left" | "right" | "new";
type Drag = { mode: DragMode; start: number; base: Fractions; moved: boolean };
type StackScale = { max: number; step: number; y: (value: number) => number };

type Geometry = {
  width: number;
  height: number;
  plotWidth: number;
  areaBottom: number;
  lineTop: number;
  lineBottom: number;
  x: (index: number) => number;
};

const MIN_POINTS = 2;
const MIN_WIDTH = 640;
const LEFT = 52;
const RIGHT = 14;
const TOP = 14;
const AREA_HEIGHT = 176;
const PANEL_GAP = 30;
const LINE_HEIGHT = 60;
const BOTTOM = 24;
const Y_TICKS = 4;
const LINE_TICKS = 2;
const X_TICKS = 6;
const MIN_BRUSH = 0.02;
const CLICK_TOLERANCE = 0.004;
const HANDLE_WIDTH = 8;
const HANDLE_HEIGHT = 28;
const TOOLTIP_OFFSET = 12;
const GRID_COLOR = "var(--color-border)";
const HANDLE_COLOR = "var(--color-accent)";

export function TimeSeriesChart(props: TimeSeriesChartProps) {
  if (props.points.length < MIN_POINTS) {
    return <EmptyState>This window holds {plural(props.points.length, "bucket")}; a chart needs at least {MIN_POINTS}.</EmptyState>;
  }
  return <ChartBody {...props} />;
}

function ChartBody({ label, points, stepMs, series, unit, line, brush = null, onBrush, onPointClick, formatTick, formatTooltipTime }: TimeSeriesChartProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const width = useWidth(wrapRef);
  const geometry = geometryOf(width, points.length, line !== undefined);
  const scale = stackScaleOf(points, series, geometry);
  const pointer = useChartPointer({ points, stepMs, brush, onBrush, onPointClick, geometry });

  return (
    <div className="max-w-full overflow-x-auto">
      <div ref={wrapRef} className="relative" style={{ minWidth: MIN_WIDTH }}>
        <svg viewBox={`0 0 ${geometry.width} ${geometry.height}`} role="img" aria-label={label} className="block h-auto w-full touch-none select-none" {...pointer.handlers}>
          <Grid geometry={geometry} max={scale.max} step={scale.step} unit={unit} />
          <StackedAreas points={points} series={series} geometry={geometry} y={scale.y} />
          {line !== undefined && <LinePanel points={points} line={line} geometry={geometry} />}
          <XTicks points={points} geometry={geometry} formatTick={formatTick} />
          {pointer.hover !== null && (
            <line x1={geometry.x(pointer.hover)} x2={geometry.x(pointer.hover)} y1={TOP} y2={geometry.lineBottom} stroke="var(--color-muted)" strokeWidth={1} />
          )}
          {pointer.selection !== null && onBrush !== undefined && <Brush selection={pointer.selection} geometry={geometry} />}
        </svg>
        {pointer.hover !== null && (
          <ChartTooltip
            point={points[pointer.hover]}
            series={series}
            line={line}
            unit={unit}
            left={(geometry.x(pointer.hover) / geometry.width) * 100}
            formatTime={formatTooltipTime}
          />
        )}
      </div>
    </div>
  );
}

function useChartPointer({ points, stepMs, brush, onBrush, onPointClick, geometry }: ChartPointerInput) {
  const [hover, setHover] = useState<number | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [draft, setDraft] = useState<Fractions | null>(null);
  const selection = draft ?? fractionsOf(brush, points);
  const fractionAt = (event: SvgPointerEvent) => {
    const box = event.currentTarget.getBoundingClientRect();
    const viewX = ((event.clientX - box.left) / box.width) * geometry.width;
    return clamp((viewX - LEFT) / geometry.plotWidth, 0, 1);
  };
  const indexAt = (fraction: number) => Math.round(fraction * (points.length - 1));
  const reset = () => {
    setDrag(null);
    setDraft(null);
  };

  const onPointerDown = (event: SvgPointerEvent) => {
    if (onBrush === undefined && onPointClick === undefined) return;
    const role = event.target instanceof Element ? event.target.getAttribute("data-brush") : null;
    const mode: DragMode = role === "left" || role === "right" || role === "body" ? role : "new";
    const start = fractionAt(event);
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrag({ mode, start, base: selection ?? [start, start], moved: false });
    setHover(null);
  };
  const onPointerMove = (event: SvgPointerEvent) => {
    const fraction = fractionAt(event);
    if (drag === null) {
      setHover(indexAt(fraction));
      return;
    }
    const moved = drag.moved || Math.abs(fraction - drag.start) > CLICK_TOLERANCE;
    if (moved !== drag.moved) setDrag({ ...drag, moved });
    if (moved && onBrush !== undefined) setDraft(dragged(drag, fraction));
  };
  const onPointerUp = (event: SvgPointerEvent) => {
    if (drag === null) return;
    const finished = draft;
    reset();
    if (!drag.moved) {
      onPointClick?.(points[indexAt(fractionAt(event))].t);
      return;
    }
    if (finished !== null) onBrush?.(bucketRange(points, indexAt(finished[0]), indexAt(finished[1]), stepMs));
  };

  return {
    hover: drag === null ? hover : null,
    selection,
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: reset, onPointerLeave: () => setHover(null) },
  };
}

function Grid({ geometry, max, step, unit }: { geometry: Geometry; max: number; step: number; unit: string }) {
  const values = Array.from({ length: Math.round(max / step) + 1 }, (_, index) => index * step);
  return (
    <g>
      {values.map((value) => {
        const lineY = geometry.areaBottom - (value / max) * AREA_HEIGHT;
        return (
          <g key={value}>
            <line x1={LEFT} x2={geometry.width - RIGHT} y1={lineY} y2={lineY} stroke={GRID_COLOR} strokeWidth={1} />
            <text x={LEFT - 8} y={lineY + 4} textAnchor="end" className="fill-subtle text-11">
              {formatNumber(value)}
            </text>
          </g>
        );
      })}
      <text x={LEFT} y={TOP - 4} className="fill-subtle text-[10px]">
        {unit}
      </text>
    </g>
  );
}

function StackedAreas({ points, series, geometry, y }: { points: readonly ChartPoint[]; series: readonly ChartSeries[]; geometry: Geometry; y: (value: number) => number }) {
  const base = points.map(() => 0);
  return (
    <g>
      {series.map((item) => {
        const lower = [...base];
        points.forEach((point, index) => {
          base[index] += point.values[item.key] ?? 0;
        });
        const top = points.map((_, index) => `${geometry.x(index).toFixed(1)},${y(base[index]).toFixed(1)}`);
        const bottom = points.map((_, index) => `${geometry.x(index).toFixed(1)},${y(lower[index]).toFixed(1)}`).reverse();
        return (
          <path
            key={item.key}
            d={`M${top.join(" L")} L${bottom.join(" L")} Z`}
            fill={item.color}
            fillOpacity={item.opacity ?? 0.9}
            stroke="var(--color-surface)"
            strokeWidth={1}
          />
        );
      })}
    </g>
  );
}

function LinePanel({ points, line, geometry }: { points: readonly ChartPoint[]; line: ChartLine; geometry: Geometry }) {
  const values = points.map((point) => point.line ?? 0);
  const step = niceStep(Math.max(...values, 1), LINE_TICKS);
  const max = step * Math.ceil(Math.max(...values, 1) / step);
  const lineY = (value: number) => geometry.lineBottom - (value / max) * LINE_HEIGHT;
  const ticks = Array.from({ length: Math.round(max / step) + 1 }, (_, index) => index * step);
  const last = points.length - 1;
  return (
    <g>
      {ticks.map((value) => (
        <g key={value}>
          <line x1={LEFT} x2={geometry.width - RIGHT} y1={lineY(value)} y2={lineY(value)} stroke={GRID_COLOR} strokeWidth={1} />
          <text x={LEFT - 8} y={lineY(value) + 4} textAnchor="end" className="fill-subtle text-11">
            {formatNumber(value)}
          </text>
        </g>
      ))}
      <text x={LEFT} y={geometry.lineTop - 8} className="fill-subtle text-[10px]">
        {line.label}, {line.unit}
      </text>
      <polyline
        points={points.map((point, index) => `${geometry.x(index).toFixed(1)},${lineY(point.line ?? 0).toFixed(1)}`).join(" ")}
        fill="none"
        stroke={line.color}
        strokeWidth={2}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      <circle cx={geometry.x(last)} cy={lineY(points[last].line ?? 0)} r={4} fill={line.color} stroke="var(--color-surface)" strokeWidth={2} />
    </g>
  );
}

function XTicks({ points, geometry, formatTick }: { points: readonly ChartPoint[]; geometry: Geometry; formatTick: (t: string) => string }) {
  const every = Math.max(1, Math.ceil(points.length / X_TICKS));
  const indices = points.map((_, index) => index).filter((index) => index % every === 0);
  return (
    <g>
      {indices.map((index) => {
        const tickX = geometry.x(index);
        const anchor = index === 0 ? "start" : tickX > geometry.width - RIGHT - 20 ? "end" : "middle";
        return (
          <g key={index}>
            <line x1={tickX} x2={tickX} y1={geometry.lineBottom} y2={geometry.lineBottom + 4} stroke="var(--color-border-strong)" />
            <text x={tickX} y={geometry.height - 6} textAnchor={anchor} className="fill-subtle text-11">
              {formatTick(points[index].t)}
            </text>
          </g>
        );
      })}
    </g>
  );
}

function Brush({ selection, geometry }: { selection: Fractions; geometry: Geometry }) {
  const [from, to] = selection;
  const left = LEFT + from * geometry.plotWidth;
  const width = Math.max(1, (to - from) * geometry.plotWidth);
  const handleY = TOP + AREA_HEIGHT / 2 - HANDLE_HEIGHT / 2;
  return (
    <g>
      <rect
        data-brush="body"
        x={left}
        y={TOP}
        width={width}
        height={geometry.lineBottom - TOP}
        fill={HANDLE_COLOR}
        fillOpacity={0.09}
        stroke={HANDLE_COLOR}
        strokeOpacity={0.55}
        className="cursor-grab"
      />
      <rect data-brush="left" x={left - HANDLE_WIDTH / 2} y={handleY} width={HANDLE_WIDTH} height={HANDLE_HEIGHT} rx={3} fill={HANDLE_COLOR} className="cursor-ew-resize" />
      <rect data-brush="right" x={left + width - HANDLE_WIDTH / 2} y={handleY} width={HANDLE_WIDTH} height={HANDLE_HEIGHT} rx={3} fill={HANDLE_COLOR} className="cursor-ew-resize" />
    </g>
  );
}

type ChartTooltipProps = {
  point: ChartPoint;
  series: readonly ChartSeries[];
  line?: ChartLine;
  unit: string;
  left: number;
  formatTime: (t: string) => string;
};

function ChartTooltip({ point, series, line, unit, left, formatTime }: ChartTooltipProps) {
  const flip = left > 60;
  return (
    <div
      className="pointer-events-none absolute top-2 z-[5] min-w-[168px] rounded-md border border-border-strong bg-overlay px-2.5 py-2 text-12 shadow-[0_8px_24px_rgba(0,0,0,0.4)]"
      style={flip ? { right: `calc(${100 - left}% + ${TOOLTIP_OFFSET}px)` } : { left: `calc(${left}% + ${TOOLTIP_OFFSET}px)` }}
    >
      <div className="mb-1 font-semibold">{formatTime(point.t)}</div>
      {[...series].reverse().map((item) => (
        <div key={item.key} className="flex items-center justify-between gap-1.5">
          <span className="inline-flex items-center gap-1.5 text-muted">
            <i className="inline-block size-2 rounded-sm" style={{ backgroundColor: item.color }} />
            {item.label}
          </span>
          <span className="font-mono">
            {formatNumber(point.values[item.key] ?? 0)} {unit}
          </span>
        </div>
      ))}
      {line !== undefined && (
        <div className="flex items-center justify-between gap-1.5">
          <span className="text-muted">{line.label}</span>
          <span className="font-mono">{point.line === null || point.line === undefined ? "—" : `${formatNumber(point.line)} ${line.unit}`}</span>
        </div>
      )}
    </div>
  );
}

function useWidth(ref: RefObject<HTMLDivElement | null>): number {
  const [width, setWidth] = useState(MIN_WIDTH);
  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.max(MIN_WIDTH, Math.floor(entry.contentRect.width))));
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return width;
}

function geometryOf(width: number, count: number, withLine: boolean): Geometry {
  const plotWidth = width - LEFT - RIGHT;
  const areaBottom = TOP + AREA_HEIGHT;
  const lineTop = withLine ? areaBottom + PANEL_GAP : areaBottom;
  const lineBottom = withLine ? lineTop + LINE_HEIGHT : areaBottom;
  return {
    width,
    height: lineBottom + BOTTOM,
    plotWidth,
    areaBottom,
    lineTop,
    lineBottom,
    x: (index) => LEFT + (index / (count - 1)) * plotWidth,
  };
}

function stackScaleOf(points: readonly ChartPoint[], series: readonly ChartSeries[], geometry: Geometry): StackScale {
  const totals = points.map((point) => series.reduce((sum, item) => sum + (point.values[item.key] ?? 0), 0));
  const peak = Math.max(...totals, 1);
  const step = niceStep(peak, Y_TICKS);
  const max = step * Math.ceil(peak / step);
  return { max, step, y: (value) => geometry.areaBottom - (value / max) * AREA_HEIGHT };
}

function bucketRange(points: readonly ChartPoint[], firstIndex: number, lastIndex: number, stepMs: number): ChartRange {
  const end = parseTimestamp(points[lastIndex].t).getTime() + stepMs;
  return { from: points[firstIndex].t, to: new Date(end).toISOString() };
}

function fractionsOf(range: ChartRange | null, points: readonly ChartPoint[]): Fractions | null {
  if (range === null) return null;
  const from = parseTimestamp(range.from).getTime();
  const to = parseTimestamp(range.to).getTime();
  const times = points.map((point) => parseTimestamp(point.t).getTime());
  const first = times.findIndex((time) => time >= from);
  const last = times.findLastIndex((time) => time < to);
  if (first < 0 || last < first) return null;
  return [first / (points.length - 1), last / (points.length - 1)];
}

function dragged(drag: Drag, fraction: number): Fractions {
  const [from, to] = drag.base;
  const delta = fraction - drag.start;
  switch (drag.mode) {
    case "body": {
      const span = to - from;
      const start = clamp(from + delta, 0, 1 - span);
      return [start, start + span];
    }
    case "left":
      return [clamp(from + delta, 0, to - MIN_BRUSH), to];
    case "right":
      return [from, clamp(to + delta, from + MIN_BRUSH, 1)];
    case "new":
      return fraction >= drag.start ? [drag.start, fraction] : [fraction, drag.start];
  }
}

function niceStep(max: number, ticks: number): number {
  const raw = max / ticks;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const normalized = raw / magnitude;
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return nice * magnitude;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
