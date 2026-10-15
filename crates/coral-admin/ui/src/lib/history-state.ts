import { useCallback, useState } from "react";
import { useLocation } from "react-router-dom";

type HistoryValues = Record<string, string>;
type PendingWrite = { entry: string; value: string | null };

const HISTORY_NAMESPACE = "coral";
const HISTORY_WRITE_DELAY_MS = 250;

const pendingWrites = new Map<string, PendingWrite>();
let writeTimer: number | null = null;

export function readHistoryValue(key: string): string | null {
  return valuesOf(window.history.state)[key] ?? null;
}

export function historyStateWith(values: HistoryValues): Record<string, unknown> {
  return { [HISTORY_NAMESPACE]: values };
}

export function currentRouterState(): Record<string, unknown> {
  const state: unknown = window.history.state;
  const usr = isRecord(state) ? state.usr : null;
  return isRecord(usr) ? usr : {};
}

export function writeHistoryValue(key: string, value: string | null): void {
  pendingWrites.set(key, { entry: entryIdentity(), value });
  if (writeTimer !== null) return;
  writeTimer = window.setTimeout(flushHistoryWrites, HISTORY_WRITE_DELAY_MS);
}

export function flushHistoryWrites(): void {
  if (writeTimer !== null) window.clearTimeout(writeTimer);
  writeTimer = null;
  const entry = entryIdentity();
  const writes = [...pendingWrites].filter(([, write]) => write.entry === entry);
  pendingWrites.clear();
  if (writes.length === 0) return;
  const values = { ...valuesOf(window.history.state) };
  for (const [key, write] of writes) {
    if (write.value === null) delete values[key];
    else values[key] = write.value;
  }
  replaceRouterState({ ...currentRouterState(), [HISTORY_NAMESPACE]: values });
}

export function useHistoryValue(key: string): readonly [string, (value: string) => void] {
  const location = useLocation();
  const [entry, setEntry] = useState(() => ({ locationKey: location.key, value: readHistoryValue(key) ?? "" }));
  const current = entry.locationKey === location.key ? entry : { locationKey: location.key, value: readHistoryValue(key) ?? "" };
  if (current !== entry) setEntry(current);

  const update = useCallback(
    (value: string) => {
      setEntry({ locationKey: location.key, value });
      writeHistoryValue(key, value === "" ? null : value);
    },
    [key, location.key],
  );

  return [current.value, update] as const;
}

function replaceRouterState(usr: Record<string, unknown>): void {
  const state: unknown = window.history.state;
  window.history.replaceState({ ...(isRecord(state) ? state : {}), usr }, "");
}

function entryIdentity(): string {
  const state: unknown = window.history.state;
  if (!isRecord(state)) return "initial";
  return `${String(state.key)}:${String(state.idx)}`;
}

function valuesOf(state: unknown): HistoryValues {
  const usr = isRecord(state) ? state.usr : null;
  const values = isRecord(usr) ? usr[HISTORY_NAMESPACE] : null;
  if (!isRecord(values)) return {};
  return Object.fromEntries(Object.entries(values).filter((entry): entry is [string, string] => typeof entry[1] === "string"));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
