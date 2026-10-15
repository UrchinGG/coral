import { useNavigate } from "react-router-dom";

import type { TrafficSeries } from "../../api/types/traffic";
import { Button } from "../../kit/Button";
import { TimeSeriesChart, type ChartPoint, type ChartRange } from "../../kit/TimeSeriesChart";
import { formatPercent } from "../../lib/format";
import { parseTimestamp, useTimeFormat, type TimeFormatOptions } from "../../lib/time";
import { CLASS_META, CLASS_SERIES, P95_LINE, SECONDS_PER_MINUTE, shareOf } from "./classes";

type TrafficChartProps = {
  series: TrafficSeries;
  selection: ChartRange | null;
  keepsSelection: boolean;
  onBrush: (range: ChartRange | null) => void;
};

const MS_PER_SECOND = 1_000;
const HOURLY_STEP_S = 3_600;

export function TrafficChart({ series, selection, keepsSelection, onBrush }: TrafficChartProps) {
  const { time } = useTimeFormat();
  const navigate = useNavigate();
  const stepMs = series.step_s * MS_PER_SECOND;
  const dated = series.step_s >= HOURLY_STEP_S;
  const openRequests = (t: string) => {
    const to = new Date(parseTimestamp(t).getTime() + stepMs).toISOString();
    navigate(`/requests?${new URLSearchParams({ from: t, to })}`);
  };
  return (
    <div className="flex flex-col gap-2">
      <Legend />
      <TimeSeriesChart
        label="Requests per minute by status class, stacked, with p95 latency below"
        points={chartPoints(series)}
        stepMs={stepMs}
        series={CLASS_SERIES}
        unit="req/min"
        line={P95_LINE}
        brush={keepsSelection ? selection : null}
        onBrush={onBrush}
        onPointClick={openRequests}
        formatTick={(t) => time(t, { seconds: false, date: dated ? "always" : "never" })}
        formatTooltipTime={(t) => time(t, { seconds: false, zone: true, date: dated ? "always" : "auto" })}
      />
      <div className="flex flex-wrap items-center justify-between gap-2 text-12 text-subtle">
        <span>
          {selectionText(series, selection, time)}
          {series.last_partial && " · the newest bucket is still filling"}
        </span>
        {keepsSelection && selection !== null && (
          <Button size="sm" variant="ghost" onClick={() => onBrush(null)}>
            Select whole window
          </Button>
        )}
      </div>
    </div>
  );
}

function Legend() {
  return (
    <div className="flex flex-wrap items-center gap-3 text-11 text-muted">
      {CLASS_META.map((meta) => (
        <span key={meta.key} className="inline-flex items-center gap-1.5">
          <i className="inline-block size-2 rounded-sm" style={{ backgroundColor: meta.color }} />
          {meta.label}
        </span>
      ))}
      <span className="inline-flex items-center gap-1.5">
        <i className="inline-block h-0.5 w-3 rounded-sm" style={{ backgroundColor: P95_LINE.color }} />
        p95 latency, lower panel
      </span>
    </div>
  );
}

function chartPoints(series: TrafficSeries): ChartPoint[] {
  const minutes = series.step_s / SECONDS_PER_MINUTE;
  return series.points.map((point) => ({
    t: point.t,
    values: Object.fromEntries(CLASS_META.map((meta) => [meta.key, point[meta.key] / minutes])),
    line: point.p95,
  }));
}

function selectionText(series: TrafficSeries, selection: ChartRange | null, time: (iso: string, options?: TimeFormatOptions) => string): string {
  if (selection === null) return "Drag across the chart to scope every table below; click a bucket to open its requests.";
  const from = parseTimestamp(selection.from).getTime();
  const to = parseTimestamp(selection.to).getTime();
  const total = series.points.reduce((sum, point) => sum + point.requests, 0);
  const selected = series.points
    .filter((point) => {
      const at = parseTimestamp(point.t).getTime();
      return at >= from && at < to;
    })
    .reduce((sum, point) => sum + point.requests, 0);
  const share = shareOf(selected, total);
  const stamp = `${time(selection.from, { seconds: false })} → ${time(selection.to, { seconds: false, zone: true })}`;
  return share === null ? `Selection ${stamp}` : `Selection ${stamp} · ${formatPercent(share, 0)} of the window's requests`;
}
