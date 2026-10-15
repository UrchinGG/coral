import type { Coverage } from "../api/types/session";
import { parseTimestamp } from "./time";

export type CoverageFeature = "log" | "detail" | "attribution" | "subjects" | "body_subjects" | "rollups" | "network";

export type CoverageWindow = {
  features: readonly CoverageFeature[];
  from: string;
  to: string;
  clampedFrom?: string | null;
};

export type CoverageNotice =
  | { kind: "not_recorded"; feature: CoverageFeature }
  | { kind: "starts_after"; feature: CoverageFeature; since: string }
  | { kind: "gap"; feature: string; from: string; to: string }
  | { kind: "clamped"; since: string }
  | { kind: "backfilled"; day: string; share: number };

export const COVERAGE_FEATURE_LABEL: Record<CoverageFeature, string> = {
  log: "Request log",
  detail: "Request headers and bodies",
  attribution: "Route, actor, cause and country",
  subjects: "Player mentions",
  body_subjects: "Batch-body player mentions",
  rollups: "Traffic rollups",
  network: "Network rollups",
};

const FEATURE_FROM: Record<CoverageFeature, keyof Coverage> = {
  log: "log_from",
  detail: "detail_from",
  attribution: "attribution_from",
  subjects: "subjects_from",
  body_subjects: "body_subjects_from",
  rollups: "rollups_from",
  network: "network_from",
};

const COMPACT_DAY = /^(\d{4})(\d{2})(\d{2})$/;
const ISO_DAY = /^\d{4}-\d{2}-\d{2}$/;
const DAY_MS = 86_400_000;

export function coverageNotices(coverage: Coverage, window: CoverageWindow): CoverageNotice[] {
  const from = parseTimestamp(window.from).getTime();
  const to = parseTimestamp(window.to).getTime();
  return [
    ...window.features.flatMap((feature) => startNotice(coverage, feature, from)),
    ...gapNotices(coverage, window.features, from, to),
    ...clampNotice(window.clampedFrom ?? null),
    ...backfillNotices(coverage, window.features, from, to),
  ];
}

export function featureSince(coverage: Coverage, feature: CoverageFeature): string | null {
  const value = coverage[FEATURE_FROM[feature]];
  return typeof value === "string" ? value : null;
}

function startNotice(coverage: Coverage, feature: CoverageFeature, from: number): CoverageNotice[] {
  const since = featureSince(coverage, feature);
  if (since === null) return [{ kind: "not_recorded", feature }];
  return parseTimestamp(since).getTime() > from ? [{ kind: "starts_after", feature, since }] : [];
}

function gapNotices(coverage: Coverage, features: readonly CoverageFeature[], from: number, to: number): CoverageNotice[] {
  return coverage.gaps
    .filter((gap) => features.some((feature) => feature === gap.feature))
    .filter((gap) => parseTimestamp(gap.from).getTime() < to && parseTimestamp(gap.to).getTime() > from)
    .map((gap): CoverageNotice => ({ kind: "gap", feature: gap.feature, from: gap.from, to: gap.to }));
}

function clampNotice(clampedFrom: string | null): CoverageNotice[] {
  return clampedFrom === null ? [] : [{ kind: "clamped", since: clampedFrom }];
}

function backfillNotices(coverage: Coverage, features: readonly CoverageFeature[], from: number, to: number): CoverageNotice[] {
  if (!features.includes("subjects")) return [];
  return Object.entries(coverage.backfill.by_name_share).flatMap(([key, share]): CoverageNotice[] => {
    const day = isoDayOf(key);
    if (day !== null && !dayOverlaps(day, from, to)) return [];
    return [{ kind: "backfilled", day: day ?? key, share }];
  });
}

function isoDayOf(key: string): string | null {
  if (ISO_DAY.test(key)) return key;
  const compact = COMPACT_DAY.exec(key);
  return compact === null ? null : `${compact[1]}-${compact[2]}-${compact[3]}`;
}

function dayOverlaps(day: string, from: number, to: number): boolean {
  const start = Date.parse(`${day}T00:00:00Z`);
  return start < to && start + DAY_MS > from;
}
