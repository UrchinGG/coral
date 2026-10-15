import { useSyncExternalStore } from "react";

import { readStored, writeStored } from "../lib/storage";

const SIDEBAR_STORAGE_KEY = "coral.sidebar.v1";
const RAIL = "rail";

const listeners = new Set<() => void>();
let rail: boolean | null = null;

export function useSidebarRail(): readonly [boolean, (rail: boolean) => void] {
  return [useSyncExternalStore(subscribe, readRail), setRail] as const;
}

function readRail(): boolean {
  rail ??= readStored(SIDEBAR_STORAGE_KEY) === RAIL;
  return rail;
}

function setRail(next: boolean): void {
  rail = next;
  writeStored(SIDEBAR_STORAGE_KEY, next ? RAIL : "full");
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
