import type { Coverage } from "../api/types/session";
import { COVERAGE_FEATURE_LABEL, coverageNotices, type CoverageFeature, type CoverageNotice, type CoverageWindow } from "../lib/coverage";
import { formatPercent } from "../lib/format";
import { useTimeFormat } from "../lib/time";
import { CoverageBanner } from "./States";

type CoverageNoticesProps = { coverage: Coverage } & CoverageWindow;

export function CoverageNotices({ coverage, ...window }: CoverageNoticesProps) {
  const { time } = useTimeFormat();
  const notices = coverageNotices(coverage, window);
  if (notices.length === 0) return null;
  const stamp = (iso: string) => time(iso, { seconds: false, date: "always", zone: true });
  return (
    <div className="flex flex-col gap-1.5">
      {notices.map((notice, index) => (
        <CoverageBanner key={index} tone={notice.kind === "gap" || notice.kind === "not_recorded" ? "warning" : "info"}>
          {noticeText(notice, stamp)}
        </CoverageBanner>
      ))}
    </div>
  );
}

function noticeText(notice: CoverageNotice, stamp: (iso: string) => string): string {
  switch (notice.kind) {
    case "not_recorded":
      return `${label(notice.feature)}: not recorded yet, so nothing in this window carries it.`;
    case "starts_after":
      return `${label(notice.feature)}: recorded since ${stamp(notice.since)}; this window starts earlier.`;
    case "gap":
      return `${label(notice.feature)}: missing from ${stamp(notice.from)} to ${stamp(notice.to)}.`;
    case "clamped":
      return `Filters on route, actor, outcome, cause, country or player start at ${stamp(notice.since)}, when they were first recorded.`;
    case "backfilled":
      return `${notice.day} was backfilled: ${formatPercent(notice.share, 0)} of name mentions matched by name only.`;
  }
}

function label(feature: CoverageFeature | string): string {
  return feature in COVERAGE_FEATURE_LABEL ? COVERAGE_FEATURE_LABEL[feature as CoverageFeature] : feature;
}
