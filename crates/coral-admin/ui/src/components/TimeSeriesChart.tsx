import { useMemo, useState, type MouseEvent } from "react";
import type { Bucket } from "../api/types";

const WIDTH = 1000;
const HEIGHT = 340;
const PAD = { l: 52, r: 16, t: 14, b: 26 };

function niceMax(v: number): number {
  if (v <= 0) return 1;
  const pow = Math.pow(10, Math.floor(Math.log10(v)));
  const f = v / pow;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * pow;
}

function fmtNum(v: number): string {
  return Math.round(v).toLocaleString();
}

function fmtTick(iso: string, hours: number): string {
  const d = new Date(iso);
  return hours <= 24
    ? d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
    : d.toLocaleString([], { month: "short", day: "numeric", hour: "2-digit" });
}

type TimeSeriesChartProps = {
  points: Bucket[];
  hours: number;
  totalLabel: string;
  onSelect?: (bucket: Bucket, index: number, points: Bucket[]) => void;
};

export function TimeSeriesChart({ points, hours, totalLabel, onSelect }: TimeSeriesChartProps) {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);

  const layout = useMemo(() => {
    const n = points.length;
    const plotW = WIDTH - PAD.l - PAD.r;
    const plotH = HEIGHT - PAD.t - PAD.b;
    const xs = points.map((_, i) => (n > 1 ? PAD.l + (i / (n - 1)) * plotW : PAD.l + plotW / 2));
    const yMax = niceMax(Math.max(1, ...points.flatMap((p) => [p.total, p.errors])));
    const y = (v: number) => PAD.t + plotH * (1 - Math.min(v, yMax) / yMax);
    return { n, plotW, plotH, xs, yMax, y };
  }, [points]);

  if (points.length === 0) {
    return (
      <div className="flex h-[220px] items-center justify-center text-sm text-gray-500">
        No {totalLabel.toLowerCase()} in this window yet
      </div>
    );
  }

  const line = (key: "total" | "errors") =>
    points.map((p, i) => `${i ? "L" : "M"}${layout.xs[i].toFixed(1)} ${layout.y(p[key]).toFixed(1)}`).join(" ");

  const area = (key: "total" | "errors") =>
    `${line(key)} L${layout.xs[layout.n - 1].toFixed(1)} ${(PAD.t + layout.plotH).toFixed(1)} L${layout.xs[0].toFixed(1)} ${(PAD.t + layout.plotH).toFixed(1)} Z`;

  const handleMove = (e: MouseEvent<SVGSVGElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const mx = ((e.clientX - rect.left) / rect.width) * WIDTH;
    const step = layout.n > 1 ? layout.plotW / (layout.n - 1) : layout.plotW;
    const i = layout.n > 1 ? Math.max(0, Math.min(layout.n - 1, Math.round((mx - PAD.l) / step))) : 0;
    setHoverIndex(i);
  };

  const hovered = hoverIndex !== null ? points[hoverIndex] : null;

  return (
    <div className="relative">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        preserveAspectRatio="none"
        className={`w-full ${onSelect ? "cursor-pointer" : ""}`}
        style={{ aspectRatio: `${WIDTH}/${HEIGHT}` }}
        onMouseMove={handleMove}
        onMouseLeave={() => setHoverIndex(null)}
        onClick={() => {
          if (onSelect && hoverIndex !== null) onSelect(points[hoverIndex], hoverIndex, points);
        }}
      >
        {[0, 1, 2, 3, 4].map((i) => {
          const y = PAD.t + layout.plotH * (1 - i / 4);
          return (
            <g key={i}>
              <line x1={PAD.l} y1={y} x2={WIDTH - PAD.r} y2={y} stroke="rgba(255,255,255,0.08)" />
              <text x={PAD.l - 8} y={y + 3.5} textAnchor="end" fontSize="11" fill="rgba(255,255,255,0.4)">
                {fmtNum((layout.yMax * i) / 4)}
              </text>
            </g>
          );
        })}
        <path d={area("total")} fill="rgba(255,255,255,0.06)" />
        <path d={line("total")} fill="none" stroke="rgba(255,255,255,0.65)" strokeWidth="1.5" />
        <path d={area("errors")} fill="rgba(248,113,113,0.13)" />
        <path d={line("errors")} fill="none" stroke="#f87171" strokeWidth="1.5" />
        {[0, 1, 2, 3, 4, 5, 6].map((k) => {
          const want = Math.min(7, layout.n);
          if (k >= want) return null;
          const i = want > 1 ? Math.round((k * (layout.n - 1)) / (want - 1)) : 0;
          return (
            <text
              key={k}
              x={layout.xs[i]}
              y={PAD.t + layout.plotH + 17}
              textAnchor="middle"
              fontSize="11"
              fill="rgba(255,255,255,0.4)"
            >
              {fmtTick(points[i].t, hours)}
            </text>
          );
        })}
        {hoverIndex !== null && (
          <>
            <line
              x1={layout.xs[hoverIndex]}
              x2={layout.xs[hoverIndex]}
              y1={PAD.t}
              y2={PAD.t + layout.plotH}
              stroke="rgba(255,255,255,0.3)"
            />
            <circle cx={layout.xs[hoverIndex]} cy={layout.y(points[hoverIndex].total)} r="3.5" fill="#fff" />
            <circle cx={layout.xs[hoverIndex]} cy={layout.y(points[hoverIndex].errors)} r="3.5" fill="#f87171" />
          </>
        )}
      </svg>
      {hovered && (
        <div className="pointer-events-none absolute top-2 left-2 rounded bg-black/80 px-2 py-1 text-xs">
          <div className="font-medium">{fmtTick(hovered.t, hours)}</div>
          <div>{fmtNum(hovered.total)} {totalLabel.toLowerCase()}</div>
          {hovered.errors > 0 && <div className="text-danger">{fmtNum(hovered.errors)} errors</div>}
          {onSelect && <div className="text-gray-400">click to inspect</div>}
        </div>
      )}
    </div>
  );
}
