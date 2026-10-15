import type { EntityKind, EntityRef } from "../api/types/refs";
import type { LookupFailure, SearchHit } from "../api/types/search";
import { REQUEST_TEXT_STATE_KEY } from "../features/requests/paths";
import { historyStateWith } from "../lib/history-state";
import { refHref, refIdentity } from "../lib/refs";
import { dashUuid, normalizeUuid, uuidV7Timestamp } from "../lib/uuid";
import type { Classification, Operator, Shape } from "./classify";

export type OmniboxItem =
  | { type: "entity"; key: string; ref: EntityRef; detail: string; tab: string | null }
  | { type: "link"; key: string; kind: EntityKind | "page" | "search"; label: string; detail: string; path: string; state: Record<string, unknown> | null };

export type OmniboxSection = { title: string; items: OmniboxItem[] };

export type EntityTarget = Extract<OmniboxItem, { type: "entity" }>;

export type PathTarget = { type: "path"; path: string; state: Record<string, unknown> | null };

export type EnterTarget = EntityTarget | PathTarget;

export const ENTER_LEAD_POINTS = 15;
export const SEARCH_STATE_KEY = "search";

const OPERATOR_KINDS: Record<Operator, readonly (EntityKind | "page" | "search")[]> = {
  player: ["player"],
  account: ["account"],
  key: ["key", "account"],
  ip: ["ip"],
  request: ["request"],
  guild: ["guild"],
  plugin: ["plugin"],
  name: ["name"],
};

export function enterTargetOf(query: string, classification: Classification, hits: readonly SearchHit[] | null): EnterTarget | null {
  if (query === "") return null;
  if (classification.url?.kind === "admin") return { type: "path", path: classification.url.path, state: null };
  const hit = hits === null ? null : enterHit(hits);
  if (hit !== null) return entityTarget(hit);
  return { type: "path", path: "/search", state: historyStateWith({ [SEARCH_STATE_KEY]: query }) };
}

export function decisiveHits(hits: readonly SearchHit[] | null, failures: readonly LookupFailure[], awaitingRemote: boolean): readonly SearchHit[] | null {
  return failures.length === 0 && !awaitingRemote ? hits : null;
}

export function enterHit(hits: readonly SearchHit[]): SearchHit | null {
  const ranked = [...hits].sort((a, b) => b.score - a.score);
  const [top, second] = ranked;
  if (top === undefined) return null;
  if (top.exact && (second === undefined || top.score - second.score >= ENTER_LEAD_POINTS)) return top;
  const tier = ranked.filter((hit) => top.score - hit.score < ENTER_LEAD_POINTS);
  const account = top.account_id;
  if (account === null || !tier.every((hit) => hit.account_id === account)) return null;
  return tier.find((hit) => isAccountPage(hit, account)) ?? { ...top, ref: { kind: "account", discord_id: account, member_id: null }, tab: null };
}

export function hitPath(hit: Pick<SearchHit, "ref" | "tab">): string | null {
  const href = refHref(hit.ref);
  if (href === null || hit.tab === null) return href;
  return `${href}${href.includes("?") ? "&" : "?"}${new URLSearchParams({ tab: hit.tab })}`;
}

export function mergeHits(local: readonly SearchHit[], remote: readonly SearchHit[]): SearchHit[] {
  const best = new Map<string, SearchHit>();
  for (const hit of [...local, ...remote]) {
    const key = hitKey(hit);
    const kept = best.get(key);
    if (kept === undefined || hit.score > kept.score) best.set(key, hit);
  }
  return [...best.values()];
}

export function hitKey(hit: Pick<SearchHit, "ref" | "tab">): string {
  return `${refIdentity(hit.ref)}:${hit.tab ?? ""}`;
}

export function failureSummary(failures: readonly LookupFailure[]): string | null {
  if (failures.length === 0) return null;
  const lookups = failures.map((failure) => `${failure.lookup} (${failure.code}: ${failure.message})`).join("; ");
  return `Some lookups failed, so these results may be incomplete: ${lookups}.`;
}

export function serverItems(hits: readonly SearchHit[], operator: Operator | null): OmniboxItem[] {
  return [...hits]
    .sort((a, b) => b.score - a.score)
    .filter((hit) => allowed(hit.ref.kind, operator))
    .map(entityTarget);
}

export function recentItems(refs: readonly EntityRef[]): OmniboxItem[] {
  return refs.map((ref) => ({ type: "entity", key: `recent:${refIdentity(ref)}`, ref, detail: "Opened earlier", tab: null }));
}

export function hintItems(classification: Classification): OmniboxItem[] {
  const { text, operator, url } = classification;
  if (text === "") return [];
  if (url?.kind === "admin") return [link("page", "Open this admin link", url.path, "Pasted admin URL", url.path)];
  const identifiers = url?.kind === "api" ? url.identifiers : null;
  const items = identifiers === null ? classification.shapes.flatMap((shape) => shapeHints(shape, text)) : identifiers.flatMap(identifierHints);
  return items.filter((item) => item.type === "link" && allowed(item.kind, operator));
}

export function searchActions(classification: Classification): OmniboxItem[] {
  const { text } = classification;
  if (text === "") return [];
  const actions: OmniboxItem[] = [];
  if (classification.shapes.includes("text")) {
    actions.push(link("search", `Search request text for "${text}"`, "/requests", "Path, query, error and user agent", "request-text", { [REQUEST_TEXT_STATE_KEY]: text }));
  }
  actions.push(link("search", "See all results", "/search", "Every interpretation, ranked", "all-results", { [SEARCH_STATE_KEY]: text }));
  return actions;
}

function entityTarget(hit: SearchHit): EntityTarget {
  return { type: "entity", key: hitKey(hit), ref: hit.ref, detail: hit.reason, tab: hit.tab };
}

function isAccountPage(hit: SearchHit, discordId: string): boolean {
  return hit.ref.kind === "account" && hit.ref.discord_id === discordId && hit.tab === null;
}

function shapeHints(shape: Shape, text: string): OmniboxItem[] {
  switch (shape) {
    case "discord_id":
      return [link("account", text, `/accounts/${text}`, "Discord id: account or Discord user")];
    case "member_id":
      return [link("account", `Member #${text}`, `/accounts/by-member/${text}`, "Coral member id")];
    case "uuid_undashed":
      return uuidHints(text, "Undashed UUID");
    case "uuid_dashed":
      return requestHint(text);
    case "guild_id":
      return [link("guild", text, `/guilds/${text}`, "Guild id")];
    case "key_prefix":
      return [link("key", text.toLowerCase(), `/keys/${text.toLowerCase()}`, "Key prefix")];
    case "ipv4":
    case "ipv6":
      return [link("ip", text, `/ips/${encodeURIComponent(text)}`, "IP address")];
    case "name":
    case "short_name":
      return [namingHint(text)];
    case "text":
      return [];
  }
}

function identifierHints(identifier: string): OmniboxItem[] {
  return normalizeUuid(identifier) === null ? [namingHint(identifier)] : uuidHints(identifier, "From the pasted API URL");
}

function uuidHints(text: string, detail: string): OmniboxItem[] {
  const uuid = normalizeUuid(text);
  if (uuid === null) return [];
  return [link("player", uuid, `/players/${uuid}`, `${detail}: player, opens even with no profile`), ...requestHint(text)];
}

function requestHint(text: string): OmniboxItem[] {
  const uuid = normalizeUuid(text);
  const ts = uuid === null ? null : uuidV7Timestamp(uuid);
  if (uuid === null || ts === null) return [];
  return [link("request", dashUuid(uuid), `/requests/${uuid}?${new URLSearchParams({ t: ts })}`, "Request id (UUIDv7)")];
}

function namingHint(name: string): OmniboxItem {
  return link("name", `Requests naming "${name}"`, `/requests?${new URLSearchParams({ player: name, include_display: "1" })}`, "Every request that names it, resolved or by name");
}

function link(
  kind: EntityKind | "page" | "search",
  label: string,
  path: string,
  detail: string,
  key: string = path,
  state: Record<string, string> | null = null,
): OmniboxItem {
  return { type: "link", key: `${kind}:${key}`, kind, label, detail, path, state: state === null ? null : historyStateWith(state) };
}

function allowed(kind: EntityKind | "page" | "search", operator: Operator | null): boolean {
  return operator === null || OPERATOR_KINDS[operator].includes(kind);
}
