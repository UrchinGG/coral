export type Tone = "neutral" | "ok" | "info" | "warning" | "limited" | "upstream" | "danger" | "accent";

export const TONE_SOFT: Record<Tone, string> = {
  neutral: "bg-surface-2 text-muted border-border-strong",
  ok: "bg-ok/12 text-ok border-ok/30",
  info: "bg-info/12 text-info border-info/30",
  warning: "bg-warning/12 text-warning border-warning/30",
  limited: "bg-limited/13 text-limited border-limited/30",
  upstream: "bg-upstream/13 text-upstream border-upstream/30",
  danger: "bg-danger/13 text-danger border-danger/30",
  accent: "bg-accent/14 text-accent-fg border-accent/32",
};

export const TONE_TEXT: Record<Tone, string> = {
  neutral: "text-muted",
  ok: "text-ok",
  info: "text-info",
  warning: "text-warning",
  limited: "text-limited",
  upstream: "text-upstream",
  danger: "text-danger",
  accent: "text-accent",
};

export const TONE_DOT: Record<Tone, string> = {
  neutral: "bg-subtle",
  ok: "bg-ok",
  info: "bg-info",
  warning: "bg-warning",
  limited: "bg-limited",
  upstream: "bg-upstream",
  danger: "bg-danger",
  accent: "bg-accent",
};

export const TONE_FILL: Record<Tone, string> = {
  neutral: "bg-subtle",
  ok: "bg-ok",
  info: "bg-info",
  warning: "bg-warning",
  limited: "bg-limited",
  upstream: "bg-upstream",
  danger: "bg-danger",
  accent: "bg-accent",
};
