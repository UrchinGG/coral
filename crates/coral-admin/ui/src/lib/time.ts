import { useCallback, useSyncExternalStore } from "react";

import { readStored, writeStored } from "./storage";

export type TimeZoneMode = "utc" | "local";

export type TimeFormatOptions = {
  seconds?: boolean;
  millis?: boolean;
  date?: "auto" | "always" | "never";
  zone?: boolean;
};

type DateParts = { year: number; month: number; day: number; hour: number; minute: number; second: number; millis: number };

const TIME_ZONE_STORAGE_KEY = "coral.tz.v1";
const FRACTION_BEYOND_MILLIS = /(\.\d{3})\d+/;
const SECOND_MS = 1_000;
const MINUTE_S = 60;
const HOUR_M = 60;
const DAY_H = 24;
const TWO_DAYS_H = 48;
const INPUT_MINUTE_LENGTH = 16;

const zoneListeners = new Set<() => void>();
let zoneMode: TimeZoneMode | null = null;

export function getTimeZoneMode(): TimeZoneMode {
  zoneMode ??= readStored(TIME_ZONE_STORAGE_KEY) === "local" ? "local" : "utc";
  return zoneMode;
}

export function setTimeZoneMode(mode: TimeZoneMode): void {
  zoneMode = mode;
  writeStored(TIME_ZONE_STORAGE_KEY, mode);
  zoneListeners.forEach((listener) => listener());
}

export function useTimeZoneMode(): TimeZoneMode {
  return useSyncExternalStore(subscribeZone, getTimeZoneMode);
}

export function useTimeFormat() {
  const mode = useTimeZoneMode();
  const time = useCallback((iso: string, options?: TimeFormatOptions) => formatTimestamp(iso, mode, options), [mode]);
  return { mode, time, relative: formatRelative };
}

export function parseTimestamp(iso: string): Date {
  return new Date(iso.replace(FRACTION_BEYOND_MILLIS, "$1"));
}

export function formatTimestamp(iso: string, mode: TimeZoneMode, options: TimeFormatOptions = {}, now = new Date()): string {
  const parts = datePartsOf(parseTimestamp(iso), mode);
  const showDate = options.date === "always" || (options.date !== "never" && dayOf(parts) !== dayOf(datePartsOf(now, mode)));
  const clock = clockOf(parts, options);
  const stamp = showDate ? `${dayOf(parts)} ${clock}` : clock;
  return options.zone ? `${stamp} ${zoneLabel(mode)}` : stamp;
}

export function formatRelative(iso: string, now = new Date()): string {
  const seconds = Math.max(0, Math.round((now.getTime() - parseTimestamp(iso).getTime()) / SECOND_MS));
  if (seconds < MINUTE_S) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / MINUTE_S);
  if (minutes < HOUR_M) return `${minutes}m ago`;
  const hours = Math.floor(minutes / HOUR_M);
  if (hours < TWO_DAYS_H) return `${hours}h ${minutes % HOUR_M}m ago`;
  return `${Math.floor(hours / DAY_H)}d ago`;
}

export function zoneLabel(mode: TimeZoneMode): string {
  return mode === "utc" ? "UTC" : "local";
}

export function toInputValue(iso: string, mode: TimeZoneMode): string {
  const parts = datePartsOf(parseTimestamp(iso), mode);
  return `${dayOf(parts)}T${pad(parts.hour)}:${pad(parts.minute)}`;
}

export function fromInputValue(value: string, mode: TimeZoneMode): string | null {
  if (value.length < INPUT_MINUTE_LENGTH) return null;
  const date = mode === "utc" ? new Date(`${value}:00Z`) : new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function subscribeZone(listener: () => void): () => void {
  zoneListeners.add(listener);
  return () => zoneListeners.delete(listener);
}

function datePartsOf(date: Date, mode: TimeZoneMode): DateParts {
  if (mode === "utc") {
    return {
      year: date.getUTCFullYear(),
      month: date.getUTCMonth() + 1,
      day: date.getUTCDate(),
      hour: date.getUTCHours(),
      minute: date.getUTCMinutes(),
      second: date.getUTCSeconds(),
      millis: date.getUTCMilliseconds(),
    };
  }
  return {
    year: date.getFullYear(),
    month: date.getMonth() + 1,
    day: date.getDate(),
    hour: date.getHours(),
    minute: date.getMinutes(),
    second: date.getSeconds(),
    millis: date.getMilliseconds(),
  };
}

function dayOf(parts: DateParts): string {
  return `${parts.year}-${pad(parts.month)}-${pad(parts.day)}`;
}

function clockOf(parts: DateParts, options: TimeFormatOptions): string {
  const minutes = `${pad(parts.hour)}:${pad(parts.minute)}`;
  if (options.seconds === false) return minutes;
  const seconds = `${minutes}:${pad(parts.second)}`;
  return options.millis ? `${seconds}.${String(parts.millis).padStart(3, "0")}` : seconds;
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}
