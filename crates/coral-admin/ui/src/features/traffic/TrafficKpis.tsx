import type { ReactNode } from "react";

import type { TrafficSummary, TrafficTotals } from "../../api/types/traffic";
import { KpiStrip, KpiTile } from "../../kit/KpiTile";
import { cx } from "../../lib/cx";
import { formatNumber, formatPercent } from "../../lib/format";
import { CLASS_META, changeOf, shareOf } from "./classes";

type DeltaProps = { current: number | null; previous: number | null; badWhenUp: boolean };

const SHARE_DIGITS = 2;
const STEADY = 0.005;

export function TrafficKpis({ summary }: { summary: TrafficSummary }) {
  const { totals, prev } = summary;
  return (
    <KpiStrip>
      <KpiTile
        label="Requests"
        value={formatNumber(totals.requests)}
        sub={prev === null ? "in scope" : <Delta current={totals.requests} previous={prev.requests} badWhenUp={false} />}
      />
      {CLASS_META.map((meta) => {
        const count = totals.classes[meta.key];
        const share = shareOf(count, totals.requests);
        return (
          <KpiTile
            key={meta.key}
            label={meta.label}
            swatch={meta.color}
            value={formatNumber(count)}
            sub={
              <>
                <span>{share === null ? "no requests" : formatPercent(share, SHARE_DIGITS)}</span>
                {prev !== null && <Delta current={count} previous={prev.classes[meta.key]} badWhenUp={meta.badWhenUp} />}
              </>
            }
          />
        );
      })}
      <LatencyTile totals={totals} prev={prev} />
    </KpiStrip>
  );
}

function LatencyTile({ totals, prev }: { totals: TrafficTotals; prev: TrafficTotals | null }) {
  return (
    <KpiTile
      label="p50 / p95"
      value={totals.p50 === null ? "—" : formatNumber(totals.p50)}
      unit={totals.p95 === null ? "ms" : `/ ${formatNumber(totals.p95)} ms`}
      sub={latencySub(totals, prev)}
    />
  );
}

function latencySub(totals: TrafficTotals, prev: TrafficTotals | null): ReactNode {
  if (prev !== null) {
    return (
      <>
        <span>p95</span>
        <Delta current={totals.p95} previous={prev.p95} badWhenUp />
      </>
    );
  }
  return totals.p99 === null ? "no requests" : `p99 ${formatNumber(totals.p99)} ms`;
}

function Delta({ current, previous, badWhenUp }: DeltaProps) {
  const change = current === null || previous === null ? null : changeOf(current, previous);
  if (change === null) return <span className="text-subtle">no earlier data</span>;
  return (
    <span className={cx("font-medium", deltaTone(change, badWhenUp))}>
      {change > 0 ? "+" : ""}
      {formatPercent(change)} vs before
    </span>
  );
}

function deltaTone(change: number, badWhenUp: boolean): string {
  if (Math.abs(change) < STEADY) return "text-muted";
  const worse = badWhenUp ? change > 0 : change < 0;
  return worse ? "text-danger" : "text-ok";
}
