import type { DetailState } from "../../api/types/request-full";
import { useSession } from "../../app/boot";
import { CoverageBanner } from "../../kit/States";
import { useTimeFormat } from "../../lib/time";

const DETAIL_RETENTION_DAYS = 14;

export function DetailStateNotice({ state }: { state: Exclude<DetailState, "present"> }) {
  const { time } = useTimeFormat();
  const since = useSession().coverage.detail_since;
  if (state === "expired") {
    return <CoverageBanner tone="warning">Detail expired (kept {DETAIL_RETENTION_DAYS} days): the headers and body of this request were stored and have since been dropped.</CoverageBanner>;
  }
  const began = since === null ? "has not started" : `began ${time(since, { seconds: false, date: "always", zone: true })}`;
  return <CoverageBanner>Detail not captured: detail logging {began}, so this request's headers and body were never stored.</CoverageBanner>;
}
