import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { pulseQuery } from "../api/traffic";
import { cx } from "../lib/cx";
import { formatNumber } from "../lib/format";

const TITLE = "Requests per minute, last 60 seconds. Opens Traffic.";
const STALE_TITLE = "The traffic rollups have not reported a recent minute. Opens Traffic.";

type PulseReading = { value: string; title: string; alarm: boolean };

export function LivePill() {
  const pulse = useQuery(pulseQuery());
  const reading = readPulse(pulse.data?.totals.rpm_now, pulse.error);
  return (
    <Link
      to="/traffic"
      title={reading.title}
      className={cx(
        "inline-flex h-7 items-center gap-[7px] rounded-[14px] border px-2.5 text-12 whitespace-nowrap text-fg max-md:px-2",
        reading.alarm ? "border-danger/30 bg-danger/13" : "border-ok/25 bg-ok/12",
      )}
    >
      <span aria-hidden className={cx("size-[7px] shrink-0 rounded-full", reading.alarm ? "bg-danger" : "animate-pulse-dot bg-ok")} />
      <b className="font-semibold">{reading.value}</b>
      <span className="max-md:hidden">req/min</span>
    </Link>
  );
}

function readPulse(rpm: number | null | undefined, error: Error | null): PulseReading {
  if (rpm === undefined && error !== null) return { value: "—", title: `Traffic pulse failed: ${error.message}`, alarm: true };
  if (rpm === undefined) return { value: "…", title: TITLE, alarm: false };
  if (rpm === null) return { value: "—", title: STALE_TITLE, alarm: true };
  return { value: formatNumber(rpm), title: TITLE, alarm: false };
}
