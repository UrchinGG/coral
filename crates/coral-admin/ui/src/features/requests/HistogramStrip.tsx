import { useQuery } from "@tanstack/react-query";

import type { RollupFilter } from "../../api/types/request-explorer";
import { ErrorState, SkeletonLines } from "../../kit/States";
import { cx } from "../../lib/cx";
import { formatNumber } from "../../lib/format";
import { useTimeFormat } from "../../lib/time";
import { histogramQuery, type HistogramPoint, type HistogramSeries } from "./histogram";

type Band = { key: keyof Omit<HistogramPoint, "t" | "requests" | "p95">; label: string; fill: string };

type HistogramStripProps = {
  filter: RollupFilter;
  unsupported: readonly string[];
  onZoom: (range: { from: string; to: string }) => void;
};

const BANDS: readonly Band[] = [
  { key: "c2xx", label: "2xx", fill: "fill-ok/55" },
  { key: "c4xx", label: "4xx", fill: "fill-warning" },
  { key: "c429_caller", label: "429 caller", fill: "fill-limited" },
  { key: "c429_upstream", label: "429 upstream", fill: "fill-upstream" },
  { key: "c5xx", label: "5xx", fill: "fill-danger" },
];
const VIEW_WIDTH = 900;
const VIEW_HEIGHT = 56;
const BAR_GAP = 1;
const MIN_BAR_HEIGHT = 0.6;
const SECOND_MS = 1_000;

export function HistogramStrip({ filter, unsupported, onZoom }: HistogramStripProps) {
  const rollupOnly = unsupported.length === 0;
  const series = useQuery({ ...histogramQuery(filter), enabled: rollupOnly });
  if (!rollupOnly) {
    return <p className="m-0 text-12 text-subtle">The histogram is drawn from the traffic rollups, which do not carry {unsupported.join(", ")} filters.</p>;
  }
  if (series.data === undefined) {
    if (series.error !== null) return <ErrorState error={series.error} onRetry={() => void series.refetch()} title="Request histogram unavailable" />;
    return <SkeletonLines widths={["100%"]} />;
  }
  return <Bars series={series.data} onZoom={onZoom} />;
}

function Bars({ series, onZoom }: { series: HistogramSeries; onZoom: (range: { from: string; to: string }) => void }) {
  const { time } = useTimeFormat();
  const { points } = series;
  if (points.length === 0) return <p className="m-0 text-12 text-subtle">No traffic in this window.</p>;
  const peak = Math.max(1, ...points.map((point) => point.requests));
  const width = VIEW_WIDTH / points.length;
  const stepMs = series.step_s * SECOND_MS;
  const zoom = (point: HistogramPoint) => onZoom({ from: point.t, to: new Date(Date.parse(point.t) + stepMs).toISOString() });
  return (
    <div className="flex flex-col gap-1">
      <svg viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`} preserveAspectRatio="none" role="img" aria-label={`Requests per ${series.step_s} s by status class, peak ${formatNumber(peak)}`} className="h-14 w-full">
        {points.map((point, index) => (
          <g key={point.t} className="cursor-zoom-in" onClick={() => zoom(point)}>
            <title>{`${time(point.t, { seconds: false })} · ${formatNumber(point.requests)} requests · ${BANDS.map((band) => `${band.label} ${formatNumber(point[band.key])}`).join(" · ")}`}</title>
            <rect x={index * width} y={0} width={width} height={VIEW_HEIGHT} fill="transparent" />
            {stack(point, peak).map((segment) => (
              <rect key={segment.band.key} x={index * width} y={segment.y} width={Math.max(width - BAR_GAP, MIN_BAR_HEIGHT)} height={segment.height} className={segment.band.fill} />
            ))}
          </g>
        ))}
      </svg>
      <div className="flex flex-wrap items-center justify-between gap-2 text-11 text-subtle">
        <span>{time(points[0].t, { seconds: false })}</span>
        <span className="flex flex-wrap gap-3">
          {BANDS.map((band) => (
            <span key={band.key} className="inline-flex items-center gap-1">
              <svg aria-hidden className="size-2">
                <rect width="8" height="8" className={band.fill} />
              </svg>
              {band.label}
            </span>
          ))}
          <span>click a bar to zoom in</span>
        </span>
        <span className={cx(series.last_partial && "italic")}>{series.last_partial ? "now (partial)" : time(points[points.length - 1].t, { seconds: false })}</span>
      </div>
    </div>
  );
}

function stack(point: HistogramPoint, peak: number): { band: Band; y: number; height: number }[] {
  let top = VIEW_HEIGHT;
  return BANDS.flatMap((band) => {
    const height = (point[band.key] / peak) * VIEW_HEIGHT;
    if (height < MIN_BAR_HEIGHT) return [];
    top -= height;
    return [{ band, y: top, height }];
  });
}
