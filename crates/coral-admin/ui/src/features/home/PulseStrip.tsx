import { useQuery } from "@tanstack/react-query";

import { pulseQuery } from "../../api/traffic";
import { pulseSeriesQuery, pulseUpstreamQuery } from "../../api/traffic-panels";
import type { TrafficSummary, UpstreamTraffic } from "../../api/types/traffic";
import { useSession } from "../../app/boot";
import { CoverageNotices } from "../../kit/CoverageNotices";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { Meter } from "../../kit/Meter";
import { Sparkline } from "../../kit/Sparkline";
import { ErrorState, Skeleton } from "../../kit/States";
import { formatNumber, formatPercent } from "../../lib/format";
import { readWriter } from "../../lib/writer";
import { CLASS_BY_KEY, rolledPoints, shareOf } from "../traffic/classes";

const SHARE_DIGITS = 2;
const PULSE_CLASSES = ["c5xx", "c429_caller", "c429_upstream"] as const;
const ROLLUP_FEATURES = ["rollups"] as const;

export function PulseStrip() {
  const summary = useQuery(pulseQuery());
  const coverage = useSession().coverage;
  if (summary.data === undefined) {
    if (summary.error !== null) return <ErrorState error={summary.error} onRetry={() => void summary.refetch()} title="Traffic pulse" />;
    return <Skeleton className="h-[96px] rounded-md" />;
  }
  return (
    <>
      <CoverageNotices coverage={coverage} features={ROLLUP_FEATURES} from={summary.data.range.from} to={summary.data.range.to} />
      <KpiStrip>
        <RateTile summary={summary.data} />
        {PULSE_CLASSES.map((key) => (
          <ClassTile key={key} summary={summary.data} classKey={key} />
        ))}
        <KpiTile
          label="p95 latency"
          value={summary.data.totals.p95 === null ? "—" : formatNumber(summary.data.totals.p95)}
          unit="ms"
          sub={summary.data.totals.p50 === null ? "no requests" : `p50 ${formatNumber(summary.data.totals.p50)} ms`}
        />
        <UpstreamTile />
        <WriterTile />
      </KpiStrip>
    </>
  );
}

function RateTile({ summary }: { summary: TrafficSummary }) {
  const series = useQuery(pulseSeriesQuery());
  const rpm = summary.totals.rpm_now;
  return (
    <KpiTile label="Requests / min" value={rpm === null ? "—" : formatNumber(rpm)} sub={rpm === null ? "rollups have not reported the last minutes" : `${formatNumber(summary.totals.requests)} in the last 60 min`}>
      {series.data !== undefined && <Sparkline values={rolledPoints(series.data).map((point) => point.requests)} color="var(--color-accent)" label="Requests per minute, last 60 minutes" />}
      {series.data === undefined && series.error !== null && <span className="text-11 text-danger">Trend failed: {series.error.message}</span>}
    </KpiTile>
  );
}

function ClassTile({ summary, classKey }: { summary: TrafficSummary; classKey: (typeof PULSE_CLASSES)[number] }) {
  const meta = CLASS_BY_KEY[classKey];
  const count = summary.totals.classes[classKey];
  const share = shareOf(count, summary.totals.requests);
  return (
    <KpiTile
      label={meta.label}
      swatch={meta.color}
      value={share === null ? "—" : formatPercent(share, SHARE_DIGITS)}
      sub={`${formatNumber(count)} in 60 min`}
    />
  );
}

function UpstreamTile() {
  const upstream = useQuery(pulseUpstreamQuery());
  if (upstream.data === undefined) {
    return <KpiTile label="Upstream" value="—" sub={upstream.error === null ? "loading" : <span className="text-danger">{upstream.error.message}</span>} />;
  }
  return <UpstreamReading upstream={upstream.data} />;
}

function UpstreamReading({ upstream }: { upstream: UpstreamTraffic }) {
  const calls = upstream.series.reduce((sum, point) => sum + point.requests, 0);
  const errors = upstream.series.reduce((sum, point) => sum + point.errors, 0);
  const share = shareOf(errors, calls);
  const capacity = upstream.capacity;
  return (
    <KpiTile
      label="Upstream errors"
      value={share === null ? "—" : formatPercent(share, SHARE_DIGITS)}
      sub={capacity === null ? `${formatNumber(errors)} of ${formatNumber(calls)} calls · capacity not reported` : `${formatNumber(errors)} of ${formatNumber(calls)} calls`}
    >
      {capacity !== null && <Meter used={capacity.used} limit={capacity.limit} label="Capacity" />}
    </KpiTile>
  );
}

function WriterTile() {
  const writer = readWriter(useSession().coverage.writer);
  return <KpiTile label="Writer" value={<span className={writer.healthy ? "text-ok" : "text-warning"}>{writer.healthy ? "Healthy" : "Check"}</span>} sub={writer.text} />;
}
