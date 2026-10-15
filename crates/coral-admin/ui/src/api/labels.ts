import { useEffect, useSyncExternalStore } from "react";

import type { LabelSpace, LabelTarget } from "../lib/refs";
import { postJson } from "./http";
import { LABEL_STALE_MS } from "./queryClient";
import type { Labels } from "./types/refs";

type LabelEntry = { name: string | null; fetchedAt: number };
type LabelBatch = Record<LabelSpace, string[]>;

export type LabelFailure = { error: Error; at: number };

const LABEL_BATCH_DELAY_MS = 10;
const LABEL_BATCH_LIMIT = 200;
const LABEL_PENDING_RETRY_MS = 4_000;
const LABEL_PENDING_ATTEMPTS = 3;
const LABEL_FAILURE_COOLDOWN_MS = 60_000;
const SPACES: readonly LabelSpace[] = ["discord", "minecraft"];

const entries: Record<LabelSpace, Map<string, LabelEntry>> = { discord: new Map(), minecraft: new Map() };
const queued: Record<LabelSpace, Set<string>> = { discord: new Set(), minecraft: new Set() };
const inFlight: Record<LabelSpace, Set<string>> = { discord: new Set(), minecraft: new Set() };
const pendingAttempts = new Map<string, number>();
const coolingUntil = new Map<string, number>();
const listeners = new Set<() => void>();

let flushTimer: number | null = null;
let lastFailure: LabelFailure | null = null;

export function useLabel(target: LabelTarget | null, eager = true): string | null {
  const space = target?.space ?? null;
  const id = target?.id ?? null;
  const name = useSyncExternalStore(subscribe, () => (space === null || id === null ? null : labelOf({ space, id })));

  useEffect(() => {
    if (space === null || id === null || !eager) return;
    requestLabel({ space, id });
  }, [space, id, eager]);

  return name;
}

export function useLabelFailure(): LabelFailure | null {
  return useSyncExternalStore(subscribe, () => lastFailure);
}

export function labelOf(target: LabelTarget): string | null {
  return entries[target.space].get(target.id)?.name ?? null;
}

export function requestLabel(target: LabelTarget): void {
  const { space, id } = target;
  if (isFresh(entries[space].get(id)) || queued[space].has(id) || inFlight[space].has(id)) return;
  if ((coolingUntil.get(attemptKey(target)) ?? 0) > Date.now()) return;
  queued[space].add(id);
  scheduleFlush(LABEL_BATCH_DELAY_MS);
}

export function absorbLabels(labels: Labels): void {
  const now = Date.now();
  for (const [id, name] of Object.entries(labels.discord)) entries.discord.set(id, { name, fetchedAt: now });
  for (const [id, name] of Object.entries(labels.minecraft)) entries.minecraft.set(id, { name, fetchedAt: now });
  retryPending(labels.pending);
  notify();
}

export function withLabels<T extends { labels: Labels }>(data: T): T {
  absorbLabels(data.labels);
  return data;
}

function scheduleFlush(delayMs: number): void {
  if (flushTimer !== null) return;
  flushTimer = window.setTimeout(flush, delayMs);
}

function flush(): void {
  flushTimer = null;
  const batch = takeBatch();
  if (batch.discord.length + batch.minecraft.length === 0) return;
  postJson<Labels>("/labels", batch).then(
    (labels) => settleBatch(batch, labels),
    (error: unknown) => failBatch(batch, error),
  );
  if (queued.discord.size + queued.minecraft.size > 0) scheduleFlush(0);
}

function takeBatch(): LabelBatch {
  const batch: LabelBatch = { discord: [], minecraft: [] };
  let room = LABEL_BATCH_LIMIT;
  for (const space of SPACES) {
    for (const id of queued[space]) {
      if (room === 0) return batch;
      queued[space].delete(id);
      inFlight[space].add(id);
      batch[space].push(id);
      room -= 1;
    }
  }
  return batch;
}

function settleBatch(batch: LabelBatch, labels: Labels): void {
  const now = Date.now();
  for (const space of SPACES) {
    const pending = new Set(labels.pending[space]);
    for (const id of batch[space]) {
      inFlight[space].delete(id);
      if (!(id in labels[space]) && !pending.has(id)) entries[space].set(id, { name: null, fetchedAt: now });
    }
  }
  lastFailure = null;
  absorbLabels(labels);
}

function failBatch(batch: LabelBatch, error: unknown): void {
  const cooldownEnd = Date.now() + LABEL_FAILURE_COOLDOWN_MS;
  for (const space of SPACES) {
    for (const id of batch[space]) {
      inFlight[space].delete(id);
      coolingUntil.set(attemptKey({ space, id }), cooldownEnd);
    }
  }
  lastFailure = { error: error instanceof Error ? error : new Error(String(error)), at: Date.now() };
  notify();
}

function retryPending(pending: Labels["pending"]): void {
  for (const space of SPACES) {
    for (const id of pending[space]) {
      const key = attemptKey({ space, id });
      const attempts = pendingAttempts.get(key) ?? 0;
      if (attempts >= LABEL_PENDING_ATTEMPTS) continue;
      pendingAttempts.set(key, attempts + 1);
      window.setTimeout(() => requestLabel({ space, id }), LABEL_PENDING_RETRY_MS * (attempts + 1));
    }
  }
}

function isFresh(entry: LabelEntry | undefined): boolean {
  return entry !== undefined && Date.now() - entry.fetchedAt < LABEL_STALE_MS;
}

function attemptKey(target: LabelTarget): string {
  return `${target.space}:${target.id}`;
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function notify(): void {
  listeners.forEach((listener) => listener());
}
