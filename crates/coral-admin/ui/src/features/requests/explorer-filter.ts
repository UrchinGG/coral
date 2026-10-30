import type { Cause, EntityRef, KeyKind } from "../../api/types/refs";
import type { GroupQuery, GroupSort, RequestQuery, RequesterGroup, RollupFilter, StatusClass } from "../../api/types/request-explorer";
import type { TimeRange } from "../../kit/TimeRangePicker";
import { isSnowflake } from "../../lib/refs";
import { normalizeUuid } from "../../lib/uuid";
import { KEY_PREFIX_CHARS, keyPrefixOf } from "./key-prefix";

export const EXPLORER_WINDOWS = ["1h", "6h", "24h", "7d", "14d"] as const;
export type ExplorerWindow = (typeof EXPLORER_WINDOWS)[number];
export const DEFAULT_WINDOW: ExplorerWindow = "1h";

export const STATUS_CLASSES: readonly StatusClass[] = ["2xx", "4xx", "5xx", "error", "rate_limited"];
export const CAUSES: readonly Cause[] = ["ok", "caller_limit", "upstream_limit", "upstream_error", "auth", "validation", "not_found", "internal", "unmatched"];
export const KEY_KINDS: readonly KeyKind[] = ["none", "personal", "developer", "internal", "session", "invalid", "unverified", "legacy", "minecraft"];
export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;
export const GROUP_SORTS: readonly GroupSort[] = ["requests", "rate_limited", "errors", "subjects"];
export const DEFAULT_GROUP_SORT: GroupSort = "requests";
export const VIEW_PARAM = "view";
export const SORT_PARAM = "sort";

export type RequesterFilter =
  | { kind: "member"; memberId: number; discordId: string | null }
  | { kind: "actor"; discordId: string }
  | { kind: "player"; uuid: string }
  | { kind: "key"; prefix: string; keyKind: KeyKind | null };

export type ExplorerFilter = {
  range: TimeRange<ExplorerWindow>;
  route: string | null;
  method: string | null;
  status: number | null;
  statusClass: StatusClass | null;
  cause: Cause | null;
  keyKind: KeyKind | null;
  requester: RequesterFilter | null;
  player: string | null;
  includeDisplay: boolean;
  ip: string | null;
  country: string | null;
};

export type WindowBounds = { from: string; to: string | null };

export type ExplorerView = "list" | "groups";

export const FILTER_PARAM_NAMES = [
  "window",
  "from",
  "to",
  "route",
  "method",
  "status",
  "class",
  "cause",
  "key_kind",
  "requester",
  "player",
  "include_display",
  "ip",
  "country",
] as const;

const WINDOW_MS: Record<ExplorerWindow, number> = {
  "1h": 3_600_000,
  "6h": 6 * 3_600_000,
  "24h": 24 * 3_600_000,
  "7d": 7 * 24 * 3_600_000,
  "14d": 14 * 24 * 3_600_000,
};
const REQUESTER_SEPARATOR = ":";
const MEMBER_ID = /^\d{1,9}$/;
const STATUS_CODE = /^[1-5]\d\d$/;
const FLAG_ON = "1";

export const EMPTY_FILTER: ExplorerFilter = {
  range: { kind: "window", window: DEFAULT_WINDOW },
  route: null,
  method: null,
  status: null,
  statusClass: null,
  cause: null,
  keyKind: null,
  requester: null,
  player: null,
  includeDisplay: false,
  ip: null,
  country: null,
};

export function filterFromParams(params: URLSearchParams): ExplorerFilter {
  return {
    range: rangeFromParams(params),
    route: params.get("route"),
    method: oneOf(params.get("method"), METHODS),
    status: statusFrom(params.get("status")),
    statusClass: oneOf(params.get("class"), STATUS_CLASSES),
    cause: oneOf(params.get("cause"), CAUSES),
    keyKind: oneOf(params.get("key_kind"), KEY_KINDS),
    requester: parseRequester(params.get("requester")),
    player: params.get("player"),
    includeDisplay: params.get("include_display") === FLAG_ON,
    ip: params.get("ip"),
    country: params.get("country"),
  };
}

export function explorerViewFrom(params: URLSearchParams): ExplorerView {
  return params.get(VIEW_PARAM) === "groups" ? "groups" : "list";
}

export function withFilterParams(base: URLSearchParams, filter: ExplorerFilter): URLSearchParams {
  const next = new URLSearchParams(base);
  FILTER_PARAM_NAMES.forEach((name) => next.delete(name));
  const set = (name: string, value: string | null) => {
    if (value !== null && value !== "") next.set(name, value);
  };
  if (filter.range.kind === "custom") {
    set("from", filter.range.from);
    set("to", filter.range.to);
  } else if (filter.range.window !== DEFAULT_WINDOW) {
    set("window", filter.range.window);
  }
  set("route", filter.route);
  set("method", filter.method);
  set("status", filter.status === null ? null : String(filter.status));
  set("class", filter.statusClass);
  set("cause", filter.cause);
  set("key_kind", filter.keyKind);
  set("requester", filter.requester === null ? null : encodeRequester(filter.requester));
  set("player", filter.player);
  set("include_display", filter.includeDisplay ? FLAG_ON : null);
  set("ip", filter.ip);
  set("country", filter.country);
  return next;
}

export function encodeRequester(requester: RequesterFilter): string {
  switch (requester.kind) {
    case "member":
      return joinParts("member", String(requester.memberId), requester.discordId);
    case "actor":
      return joinParts("actor", requester.discordId, null);
    case "player":
      return joinParts("player", requester.uuid, null);
    case "key":
      return joinParts("key", requester.prefix, requester.keyKind);
  }
}

export function parseRequester(raw: string | null): RequesterFilter | null {
  if (raw === null) return null;
  const [kind, id = "", extra = ""] = raw.split(REQUESTER_SEPARATOR);
  if (kind === "member" && MEMBER_ID.test(id)) return { kind, memberId: Number(id), discordId: isSnowflake(extra) ? extra : null };
  if (kind === "actor" && isSnowflake(id)) return { kind, discordId: id };
  if (kind === "player" && normalizeUuid(id) !== null) return { kind, uuid: id };
  if (kind === "key" && id.length === KEY_PREFIX_CHARS) return { kind, prefix: id, keyKind: oneOf(extra, KEY_KINDS) };
  return null;
}

export function describeRequester(requester: RequesterFilter): string {
  switch (requester.kind) {
    case "member":
      return `member #${requester.memberId}`;
    case "actor":
      return `Discord user ${requester.discordId}`;
    case "player":
      return `Minecraft account ${requester.uuid}`;
    case "key":
      return `key prefix ${requester.prefix}`;
  }
}

export function requesterFromText(text: string): RequesterFilter | null {
  const value = text.trim();
  if (isSnowflake(value)) return { kind: "actor", discordId: value };
  if (MEMBER_ID.test(value)) return { kind: "member", memberId: Number(value), discordId: null };
  const uuid = normalizeUuid(value);
  if (uuid !== null) return { kind: "player", uuid };
  if (value.length >= KEY_PREFIX_CHARS) return { kind: "key", prefix: keyPrefixOf(value), keyKind: null };
  return null;
}

export function requesterFromRef(ref: EntityRef): RequesterFilter | null {
  if (ref.kind === "account") {
    return ref.member_id === null ? { kind: "actor", discordId: ref.discord_id } : { kind: "member", memberId: ref.member_id, discordId: ref.discord_id };
  }
  if (ref.kind === "player") return { kind: "player", uuid: ref.uuid };
  if (ref.kind === "key") return { kind: "key", prefix: ref.prefix, keyKind: ref.key_kind };
  return null;
}

export function requesterRef(requester: RequesterFilter): EntityRef | null {
  switch (requester.kind) {
    case "member":
      return requester.discordId === null ? null : { kind: "account", discord_id: requester.discordId, member_id: requester.memberId };
    case "actor":
      return { kind: "account", discord_id: requester.discordId, member_id: null };
    case "player":
      return { kind: "player", uuid: requester.uuid };
    case "key":
      return requester.keyKind === null ? null : { kind: "key", key_id: null, prefix: requester.prefix, key_kind: requester.keyKind };
  }
}

export function groupSelection(group: RequesterGroup): Pick<ExplorerFilter, "requester" | "keyKind"> {
  const requester = requesterFromRef(group.requester);
  if (group.requester.kind === "account") return { requester, keyKind: null };
  const actor = group.actor === null ? null : requesterFromRef(group.actor);
  return { requester: requester ?? actor, keyKind: group.key_kind };
}

export function windowBounds(range: TimeRange<ExplorerWindow>, anchorMs: number): WindowBounds {
  if (range.kind === "custom") return { from: range.from, to: range.to };
  return { from: new Date(anchorMs - WINDOW_MS[range.window]).toISOString(), to: null };
}

export function rollupFilterOf(filter: ExplorerFilter, bounds: WindowBounds): RollupFilter {
  return compact({
    from: bounds.from,
    to: bounds.to ?? undefined,
    route: filter.route ?? undefined,
    method: filter.method ?? undefined,
    status: filter.status ?? undefined,
    class: filter.statusClass ?? undefined,
    cause: filter.cause ?? undefined,
    key_kind: filter.keyKind ?? undefined,
    ...requesterFields(filter.requester),
  });
}

export function requestQueryOf(filter: ExplorerFilter, q: string, bounds: WindowBounds): RequestQuery {
  return compact({
    ...rollupFilterOf(filter, bounds),
    ip: filter.ip ?? undefined,
    country: filter.country ?? undefined,
    player: filter.player ?? undefined,
    include_display: filter.player !== null && filter.includeDisplay ? true : undefined,
    q: q.trim() === "" ? undefined : q.trim(),
  });
}

export function histogramFilterOf(filter: ExplorerFilter, bounds: WindowBounds, anchorMs: number): RollupFilter {
  return rollupFilterOf(filter, closedBounds(bounds, anchorMs));
}

export function groupQueryOf(filter: ExplorerFilter, bounds: WindowBounds, anchorMs: number, sort: GroupSort): GroupQuery {
  return compact({ ...rollupFilterOf(filter, closedBounds(bounds, anchorMs)), sort });
}

export function groupSortFrom(params: URLSearchParams): GroupSort {
  return oneOf(params.get(SORT_PARAM), GROUP_SORTS) ?? DEFAULT_GROUP_SORT;
}

function unrolledFilters(filter: ExplorerFilter, q: string): string[] {
  const unrolled: [string, boolean][] = [
    ["player", filter.player !== null],
    ["IP", filter.ip !== null],
    ["country", filter.country !== null],
    ["text", q.trim() !== ""],
  ];
  return unrolled.filter(([, set]) => set).map(([name]) => name);
}

export function groupGaps(filter: ExplorerFilter, q: string): string[] {
  return [...unrolledFilters(filter, q), ...unrolledRequester(filter)];
}

export function histogramGaps(filter: ExplorerFilter, q: string): string[] {
  const seriesGaps: [string, boolean][] = [
    ["method", filter.method !== null],
    ["status code", filter.status !== null],
  ];
  return [...unrolledFilters(filter, q), ...seriesGaps.filter(([, set]) => set).map(([name]) => name), ...unrolledRequester(filter)];
}

function unrolledRequester(filter: ExplorerFilter): string[] {
  switch (filter.requester?.kind) {
    case "key":
      return ["key prefix"];
    case "player":
      return ["Minecraft account"];
    default:
      return [];
  }
}

export function parseStatusCode(text: string): number | null {
  return statusFrom(text.trim());
}

function joinParts(kind: string, id: string, extra: string | null): string {
  return [kind, id, ...(extra === null ? [] : [extra])].join(REQUESTER_SEPARATOR);
}

function closedBounds(bounds: WindowBounds, anchorMs: number): { from: string; to: string } {
  return { from: bounds.from, to: bounds.to ?? new Date(anchorMs).toISOString() };
}

function requesterFields(requester: RequesterFilter | null): Pick<RollupFilter, "member" | "actor" | "key_prefix"> {
  if (requester === null) return {};
  switch (requester.kind) {
    case "member":
      return { member: requester.memberId };
    case "actor":
      return { actor: requester.discordId };
    case "player":
      return { actor: requester.uuid };
    case "key":
      return { key_prefix: requester.prefix };
  }
}

function rangeFromParams(params: URLSearchParams): TimeRange<ExplorerWindow> {
  const from = params.get("from");
  const to = params.get("to");
  if (from !== null && to !== null && Date.parse(from) < Date.parse(to)) return { kind: "custom", from, to };
  return { kind: "window", window: oneOf(params.get("window"), EXPLORER_WINDOWS) ?? DEFAULT_WINDOW };
}

function statusFrom(raw: string | null): number | null {
  return raw !== null && STATUS_CODE.test(raw) ? Number(raw) : null;
}

function oneOf<T extends string>(raw: string | null, values: readonly T[]): T | null {
  return values.find((value) => value === raw) ?? null;
}

function compact<T extends object>(value: T): T {
  return Object.fromEntries(Object.entries(value).filter(([, field]) => field !== undefined)) as T;
}
