import type { ReactNode } from "react";

import type { Cause } from "../api/types/refs";
import { cx } from "../lib/cx";
import { CAUSE_LABEL, CAUSE_TONE, METHOD_TONE, type BadgeVariant } from "./badge-styles";
import { TONE_DOT, TONE_SOFT, TONE_TEXT, type Tone } from "./tone";

type PillProps = { tone?: Tone; dot?: boolean; children: ReactNode; title?: string };
type BadgeProps = { variant?: BadgeVariant; icon?: ReactNode; children: ReactNode; title?: string };
type StatusPillProps = { status: number; outcomeStatus: number | null; cause: Cause };

const BADGE_VARIANTS: Record<BadgeVariant, string> = {
  neutral: "border-border-strong bg-surface-2 text-muted",
  cf: "border-cf/35 bg-cf/8 text-cf",
  byname: "border-warning/32 bg-warning/12 text-warning",
  bot: "border-entity-account/32 bg-entity-account/8 text-entity-account",
  key: "border-entity-key/30 bg-entity-key/7 text-entity-key",
  danger: "border-danger/32 bg-danger/10 text-danger",
};

export function Pill({ tone = "neutral", dot = true, children, title }: PillProps) {
  return (
    <span
      title={title}
      className={cx("inline-flex h-5 items-center gap-1.5 rounded-full border px-[7px] text-11 font-semibold whitespace-nowrap", TONE_SOFT[tone])}
    >
      {dot && <span className={cx("size-1.5 shrink-0 rounded-full", TONE_DOT[tone])} />}
      {children}
    </span>
  );
}

export function Badge({ variant = "neutral", icon, children, title }: BadgeProps) {
  return (
    <span
      title={title}
      className={cx(
        "inline-flex h-[18px] items-center gap-1 rounded-[3px] border px-[5px] align-middle text-[10.5px] font-semibold tracking-[0.02em] whitespace-nowrap",
        BADGE_VARIANTS[variant],
      )}
    >
      {icon}
      {children}
    </span>
  );
}

export function MethodBadge({ method }: { method: string }) {
  const tone = METHOD_TONE[method.toUpperCase()] ?? "neutral";
  return (
    <span
      className={cx(
        "inline-flex h-[18px] min-w-11 items-center justify-center rounded-[3px] px-[5px] font-mono text-[10.5px] font-medium tracking-[0.02em]",
        TONE_SOFT[tone],
      )}
    >
      {method.toUpperCase()}
    </span>
  );
}

export function StatusPill({ status, outcomeStatus, cause }: StatusPillProps) {
  const tone = CAUSE_TONE[cause];
  const codeTone: Tone = outcomeStatus === null ? tone : "ok";
  return (
    <span className="inline-flex items-center gap-[5px] text-12 whitespace-nowrap">
      <span className={cx("font-mono font-medium", TONE_TEXT[codeTone])}>{status}</span>
      {outcomeStatus !== null && (
        <>
          <span className="text-subtle">→</span>
          <span className={cx("font-mono font-medium", TONE_TEXT[tone])}>{outcomeStatus}</span>
        </>
      )}
      <span className="text-subtle">·</span>
      <span className={TONE_TEXT[tone]}>{CAUSE_LABEL[cause]}</span>
    </span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-b-2 border-border-strong bg-surface-2 px-[5px] font-mono text-11 leading-4 whitespace-nowrap text-muted">
      {children}
    </kbd>
  );
}
