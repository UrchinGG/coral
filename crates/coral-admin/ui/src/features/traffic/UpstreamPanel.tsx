import type { ReactNode } from "react";

import type { UpstreamTraffic } from "../../api/types/traffic";
import { KeyValue } from "../../kit/KeyValue";
import { Meter } from "../../kit/Meter";
import { Sparkline } from "../../kit/Sparkline";
import { CoverageBanner } from "../../kit/States";
import { formatNumber, formatPercent } from "../../lib/format";
import { parseTimestamp, useTimeFormat } from "../../lib/time";
import { shareOf } from "./classes";

const SPARK_HEIGHT = 36;
const SECONDS_PER_MINUTE = 60;

export function UpstreamPanel({ upstream }: { upstream: UpstreamTraffic }) {
  const { time, relative } = useTimeFormat();
  const requests = upstream.series.reduce((sum, point) => sum + point.requests, 0);
  const errors = upstream.series.reduce((sum, point) => sum + point.errors, 0);
  const first = upstream.snapshots.reduce((sum, point) => sum + point.first, 0);
  const changed = upstream.snapshots.reduce((sum, point) => sum + point.delta, 0);
  const errorShare = shareOf(errors, requests);
  const minutes = upstream.background.window_s / SECONDS_PER_MINUTE;
  const truncated = parseTimestamp(upstream.from).getTime() < parseTimestamp(upstream.history_from).getTime();
  return (
    <div className="flex flex-col gap-3.5">
      {truncated && (
        <CoverageBanner>Per-minute upstream history starts {time(upstream.history_from, { seconds: false, date: "always", zone: true })}; earlier minutes have expired.</CoverageBanner>
      )}
      <KeyValue
        items={[
          { label: "Upstream calls", value: <b>{formatNumber(requests)}</b> },
          { label: "Upstream errors", value: <>{formatNumber(errors)}{errorShare !== null && <span className="text-muted"> · {formatPercent(errorShare)}</span>}</> },
          { label: "Snapshots stored", value: <>{formatNumber(first + changed)}<span className="text-muted"> · {formatNumber(first)} first-time, {formatNumber(changed)} changed</span></> },
        ]}
      />
      <div>
        <div className="mb-1 text-11 font-semibold tracking-[0.06em] text-subtle uppercase">Upstream calls per {upstream.step_s / SECONDS_PER_MINUTE} min</div>
        <Sparkline values={upstream.series.map((point) => point.requests)} color="var(--color-upstream)" height={SPARK_HEIGHT} label="Upstream calls over the scope" />
        <div className="mt-0.5 flex justify-between text-11 text-subtle">
          <span>{time(upstream.from, { seconds: false })}</span>
          <span>{time(upstream.to, { seconds: false, zone: true })}</span>
        </div>
      </div>
      <BackgroundMeter upstream={upstream} minutes={minutes} />
      {upstream.capacity === null ? (
        <Reading label="Capacity">not reported; the upstream client has published no capacity in the last minute</Reading>
      ) : (
        <div className="flex flex-col gap-1">
          <Meter used={upstream.capacity.used} limit={upstream.capacity.limit} label="Capacity" />
          <span className="text-11 text-subtle">reported {relative(upstream.capacity.at)}</span>
        </div>
      )}
    </div>
  );
}

function BackgroundMeter({ upstream, minutes }: { upstream: UpstreamTraffic; minutes: number }) {
  const { used, limit } = upstream.background;
  if (limit === null) {
    return <Reading label="Background refresh">{formatNumber(used)} used per {minutes} min; its share is unknown until capacity is reported</Reading>;
  }
  return <Meter used={used} limit={limit} label="Background refresh share" unit={`per ${minutes} min`} />;
}

function Reading({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5 text-12">
      <span>{label}</span>
      <span className="text-muted">{children}</span>
    </div>
  );
}
