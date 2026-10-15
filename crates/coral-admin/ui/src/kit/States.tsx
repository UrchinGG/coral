import { Info, RefreshCw, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";

import { isApiError } from "../api/http";
import { cx } from "../lib/cx";
import { useTimeFormat } from "../lib/time";
import { Button } from "./Button";

type SkeletonProps = { className?: string };
type SkeletonLinesProps = { widths?: readonly string[] };
type SkeletonRowsProps = { count?: number };
type EmptyStateProps = { children: ReactNode; action?: ReactNode };
type ErrorStateProps = { error: unknown; onRetry?: () => void; retrying?: boolean; title?: string };
type CoverageBannerProps = { tone?: "info" | "warning"; children: ReactNode };

const DEFAULT_LINE_WIDTHS = ["40%", "70%", "55%"] as const;
const DEFAULT_ROW_COUNT = 8;

export function Skeleton({ className }: SkeletonProps) {
  return <div aria-hidden className={cx("skeleton", className)} />;
}

export function SkeletonLines({ widths = DEFAULT_LINE_WIDTHS }: SkeletonLinesProps) {
  return (
    <div className="flex flex-col gap-1.5" aria-busy>
      {widths.map((width, index) => (
        <div key={index} aria-hidden className="skeleton h-3" style={{ width }} />
      ))}
    </div>
  );
}

export function SkeletonRows({ count = DEFAULT_ROW_COUNT }: SkeletonRowsProps) {
  return (
    <div aria-busy>
      {Array.from({ length: count }, (_, index) => (
        <div key={index} aria-hidden className="relative h-row border-b border-border last:border-b-0">
          <div className="skeleton absolute top-[9px] right-[30%] left-3 h-2.5 rounded-[3px]" />
        </div>
      ))}
    </div>
  );
}

export function RefetchBar() {
  return (
    <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 z-[2] h-0.5 overflow-hidden rounded-t-md">
      <div className="h-full w-2/5 animate-indeterminate bg-accent" />
    </div>
  );
}

export function EmptyState({ children, action }: EmptyStateProps) {
  return (
    <div className="flex flex-wrap items-center gap-3 px-3.5 py-[18px] text-12 text-muted">
      <span>{children}</span>
      {action}
    </div>
  );
}

export function ErrorState({ error, onRetry, retrying = false, title }: ErrorStateProps) {
  const { time } = useTimeFormat();
  const message = error instanceof Error ? error.message : String(error);
  const meta = isApiError(error)
    ? [
        `${error.status === 0 ? "network" : error.status} ${error.code}`,
        error.serverTime === null ? null : `server time ${serverTimeText(error.serverTime, time)}`,
        error.serverMs === null ? null : `${Math.round(error.serverMs)} ms`,
        error.endpoint,
      ]
    : ["client error"];
  return (
    <div role="alert" className="flex flex-col items-start gap-2 rounded-md border border-dashed border-danger/40 bg-danger/5 p-3.5">
      <div className="flex items-center gap-2 font-semibold text-danger">
        <TriangleAlert className="size-3.5 shrink-0" />
        <span className="wrap-anywhere">{title === undefined ? message : `${title}: ${message}`}</span>
      </div>
      <div className="font-mono text-12 wrap-anywhere text-muted">{meta.filter((part) => part !== null).join(" · ")}</div>
      {onRetry !== undefined && (
        <Button size="sm" onClick={onRetry} pending={retrying} icon={<RefreshCw className="size-3" />}>
          Retry
        </Button>
      )}
    </div>
  );
}

function serverTimeText(header: string, time: (iso: string, options: { zone: boolean; date: "always" }) => string): string {
  const date = new Date(header);
  return Number.isNaN(date.getTime()) ? header : time(date.toISOString(), { zone: true, date: "always" });
}

export function CoverageBanner({ tone = "info", children }: CoverageBannerProps) {
  const Icon = tone === "warning" ? TriangleAlert : Info;
  return (
    <div
      role="note"
      className={cx(
        "flex items-start gap-2 rounded-md border px-3 py-[7px] text-12",
        tone === "warning" ? "border-warning/24 bg-warning/12 text-[#f5dea0]" : "border-info/22 bg-info/12 text-[#c5dafc]",
      )}
    >
      <Icon className={cx("mt-0.5 size-3.5 shrink-0", tone === "warning" ? "text-warning" : "text-info")} />
      <span className="min-w-0 wrap-anywhere">{children}</span>
    </div>
  );
}
