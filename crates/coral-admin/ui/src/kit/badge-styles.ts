import type { Cause } from "../api/types/refs";
import type { Tone } from "./tone";

export type BadgeVariant = "neutral" | "cf" | "byname" | "bot" | "key" | "danger";

export const CAUSE_LABEL: Record<Cause, string> = {
  ok: "ok",
  caller_limit: "caller limit",
  upstream_limit: "upstream limit",
  upstream_error: "upstream error",
  auth: "auth",
  validation: "validation",
  not_found: "not found",
  internal: "internal",
  unmatched: "unmatched",
};

export const CAUSE_TONE: Record<Cause, Tone> = {
  ok: "ok",
  caller_limit: "limited",
  upstream_limit: "upstream",
  upstream_error: "danger",
  auth: "warning",
  validation: "warning",
  not_found: "info",
  internal: "danger",
  unmatched: "info",
};

export const METHOD_TONE: Record<string, Tone> = {
  GET: "info",
  POST: "accent",
  PUT: "warning",
  PATCH: "warning",
  DELETE: "danger",
};
