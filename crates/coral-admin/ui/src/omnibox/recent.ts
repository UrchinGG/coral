import { useEffect, useEffectEvent, useSyncExternalStore } from "react";

import type { EntityRef, KeyKind } from "../api/types/refs";
import { refIdentity } from "../lib/refs";
import { readStored, writeStored } from "../lib/storage";

const RECENT_STORAGE_KEY = "coral.recent.v1";
export const RECENT_LIMIT = 8;

const KEY_KINDS: readonly KeyKind[] = ["none", "personal", "developer", "internal", "session", "invalid", "unverified", "legacy"];

const listeners = new Set<() => void>();
let recent: readonly EntityRef[] | null = null;

export function useRecent(): readonly EntityRef[] {
  return useSyncExternalStore(subscribe, readRecent);
}

export function useRecordRecent(ref: EntityRef | null): void {
  const identity = ref === null ? null : refIdentity(ref);
  const record = useEffectEvent(() => {
    if (ref !== null) pushRecent(ref);
  });
  useEffect(() => {
    if (identity !== null) record();
  }, [identity]);
}

export function readRecent(): readonly EntityRef[] {
  recent ??= parseRecent(readStored(RECENT_STORAGE_KEY));
  return recent;
}

export function pushRecent(ref: EntityRef): void {
  const stored = toStoredRef(ref);
  if (stored === null) return;
  const identity = refIdentity(stored);
  recent = [stored, ...readRecent().filter((entry) => refIdentity(entry) !== identity)].slice(0, RECENT_LIMIT);
  writeStored(RECENT_STORAGE_KEY, recent);
  listeners.forEach((listener) => listener());
}

export function parseRecent(value: unknown): EntityRef[] {
  if (!Array.isArray(value)) return [];
  return value.map(parseRef).filter((ref): ref is EntityRef => ref !== null).slice(0, RECENT_LIMIT);
}

export function toStoredRef(ref: EntityRef): EntityRef | null {
  switch (ref.kind) {
    case "account":
      return { kind: "account", discord_id: ref.discord_id, member_id: ref.member_id };
    case "player":
      return { kind: "player", uuid: ref.uuid };
    case "name":
      return { kind: "name", name: ref.name };
    case "request":
      return { kind: "request", request_id: ref.request_id, ts: ref.ts };
    case "ip":
      return { kind: "ip", ip: ref.ip, country: ref.country, proxied: ref.proxied };
    case "key":
      return { kind: "key", key_id: ref.key_id, prefix: ref.prefix, key_kind: ref.key_kind };
    case "guild":
      return { kind: "guild", guild_id: ref.guild_id, name: ref.name };
    case "plugin":
      return { kind: "plugin", slug: ref.slug, name: ref.name };
    case "internal":
    case "anonymous":
      return null;
  }
}

function parseRef(value: unknown): EntityRef | null {
  if (typeof value !== "object" || value === null || !("kind" in value)) return null;
  const entry = value as Record<string, unknown>;
  switch (entry.kind) {
    case "account":
      return isString(entry.discord_id) && isNumberOrNull(entry.member_id)
        ? { kind: "account", discord_id: entry.discord_id, member_id: entry.member_id }
        : null;
    case "player":
      return isString(entry.uuid) ? { kind: "player", uuid: entry.uuid } : null;
    case "name":
      return isString(entry.name) ? { kind: "name", name: entry.name } : null;
    case "request":
      return isString(entry.request_id) && isString(entry.ts) ? { kind: "request", request_id: entry.request_id, ts: entry.ts } : null;
    case "ip":
      return isString(entry.ip) && isStringOrNull(entry.country) && typeof entry.proxied === "boolean"
        ? { kind: "ip", ip: entry.ip, country: entry.country, proxied: entry.proxied }
        : null;
    case "key":
      return isNumberOrNull(entry.key_id) && isString(entry.prefix) && isKeyKind(entry.key_kind)
        ? { kind: "key", key_id: entry.key_id, prefix: entry.prefix, key_kind: entry.key_kind }
        : null;
    case "guild":
      return isString(entry.guild_id) && isStringOrNull(entry.name) ? { kind: "guild", guild_id: entry.guild_id, name: entry.name } : null;
    case "plugin":
      return isString(entry.slug) && isStringOrNull(entry.name) ? { kind: "plugin", slug: entry.slug, name: entry.name } : null;
    default:
      return null;
  }
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function isStringOrNull(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function isNumberOrNull(value: unknown): value is number | null {
  return value === null || typeof value === "number";
}

function isKeyKind(value: unknown): value is KeyKind {
  return KEY_KINDS.some((kind) => kind === value);
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
