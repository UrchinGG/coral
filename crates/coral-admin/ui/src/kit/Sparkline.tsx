type SparklineProps = {
  values: readonly number[];
  color: string;
  height?: number;
  label?: string;
};

const WIDTH = 160;
const DEFAULT_HEIGHT = 30;
const PADDING = 3;
const HEADROOM = 1.05;
const FLOOR = 0.9;
const AREA_OPACITY = 0.1;

export function Sparkline({ values, color, height = DEFAULT_HEIGHT, label }: SparklineProps) {
  if (values.length < 2) return null;
  const max = Math.max(...values) * HEADROOM;
  const min = Math.min(...values) * FLOOR;
  const x = (index: number) => PADDING + (index / (values.length - 1)) * (WIDTH - PADDING * 2);
  const y = (value: number) => height - PADDING - ((value - min) / (max - min || 1)) * (height - PADDING * 2);
  const points = values.map((value, index) => `${x(index).toFixed(1)},${y(value).toFixed(1)}`);
  const last = values.length - 1;
  return (
    <svg
      viewBox={`0 0 ${WIDTH} ${height}`}
      preserveAspectRatio="none"
      role={label === undefined ? undefined : "img"}
      aria-label={label}
      aria-hidden={label === undefined || undefined}
      className="block w-full"
      style={{ height }}
    >
      <path d={`M${points.join(" L")} L${x(last).toFixed(1)},${height} L${x(0).toFixed(1)},${height} Z`} fill={color} fillOpacity={AREA_OPACITY} />
      <polyline points={points.join(" ")} fill="none" stroke={color} strokeWidth={1.6} strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
      <path d={`M${x(last).toFixed(1)},${y(values[last]).toFixed(1)} h0`} stroke={color} strokeWidth={6} strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}
