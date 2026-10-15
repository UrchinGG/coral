import { isDashedUuid, isUndashedUuid, isUuid, normalizeUuid } from "../lib/uuid";

export type Operator = "player" | "account" | "key" | "ip" | "request" | "guild" | "plugin" | "name";

export type Shape =
  | "discord_id"
  | "member_id"
  | "uuid_undashed"
  | "uuid_dashed"
  | "guild_id"
  | "key_prefix"
  | "ipv4"
  | "ipv6"
  | "name"
  | "short_name"
  | "text";

export type PastedUrl = { kind: "admin"; path: string } | { kind: "api"; identifiers: string[]; carriesKey: boolean };

export type Classification = {
  text: string;
  operator: Operator | null;
  shapes: Shape[];
  url: PastedUrl | null;
};

const OPERATORS: Record<string, Operator> = {
  p: "player",
  a: "account",
  k: "key",
  ip: "ip",
  r: "request",
  g: "guild",
  pl: "plugin",
  n: "name",
};

const OPERATOR_PREFIX = /^(pl|ip|p|a|k|r|g|n):\s*/i;
const MENTION = /^<@!?(\d+)>$/;
const SURROUNDING_QUOTES = /^["'`]+|["'`]+$/g;
const URL_SCHEME = /^https?:\/\//i;
const DISCORD_ID = /^\d{17,20}$/;
const MEMBER_ID = /^\d{1,9}$/;
const GUILD_ID = /^[0-9a-f]{24}$/i;
const KEY_PREFIX = /^[0-9a-f]{8}$/i;
const IPV4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/;
const IPV6 = /^(?=.*:)[0-9a-f:.]{2,45}$/i;
const NAME = /^[A-Za-z0-9_]{3,25}$/;
const SHORT_NAME = /^[A-Za-z0-9_]{1,2}$/;
const IDENTIFIER_PARAMS = ["uuid", "player", "name", "uuids", "names"] as const;
const NAME_SEGMENT_PARENTS = new Set(["resolve"]);

const SHAPE_LABEL: Record<Shape, string> = {
  discord_id: "Discord id",
  member_id: "member id",
  uuid_undashed: "undashed UUID",
  uuid_dashed: "dashed UUID",
  guild_id: "guild id",
  key_prefix: "key prefix",
  ipv4: "IPv4",
  ipv6: "IPv6",
  name: "name",
  short_name: "short name",
  text: "text",
};

const OPERATOR_LABEL: Record<Operator, string> = {
  player: "player",
  account: "account",
  key: "key",
  ip: "IP",
  request: "request",
  guild: "guild",
  plugin: "plugin",
  name: "raw name",
};

export function normalizeQuery(raw: string): string {
  const trimmed = raw.trim();
  const mention = MENTION.exec(trimmed);
  if (mention !== null) return mention[1];
  return trimmed.replace(SURROUNDING_QUOTES, "").trim();
}

export function classify(raw: string, adminOrigin: string): Classification {
  const normalized = normalizeQuery(raw);
  const operatorMatch = OPERATOR_PREFIX.exec(normalized);
  const operator = operatorMatch === null ? null : OPERATORS[operatorMatch[1].toLowerCase()];
  const text = operatorMatch === null ? normalized : normalizeQuery(normalized.slice(operatorMatch[0].length));
  const url = URL_SCHEME.test(text) ? parsePastedUrl(text, adminOrigin) : null;
  return { text, operator, shapes: url === null ? shapesOf(text) : [], url };
}

export function describeClassification(classification: Classification): string {
  if (classification.text === "") return classification.operator === null ? "" : `type forced: ${OPERATOR_LABEL[classification.operator]}`;
  const parts: string[] = [];
  if (classification.operator !== null) parts.push(`forced: ${OPERATOR_LABEL[classification.operator]}`);
  if (classification.url?.kind === "admin") parts.push("admin link");
  if (classification.url?.kind === "api") parts.push(apiUrlLabel(classification.url));
  if (classification.url === null) parts.push(classification.shapes.map((shape) => SHAPE_LABEL[shape]).join(" or "));
  return parts.join(" · ");
}

function shapesOf(text: string): Shape[] {
  if (text === "") return [];
  if (DISCORD_ID.test(text)) return ["discord_id"];
  if (KEY_PREFIX.test(text) && MEMBER_ID.test(text)) return ["member_id", "key_prefix", "name"];
  if (MEMBER_ID.test(text)) return SHORT_NAME.test(text) ? ["member_id", "short_name"] : ["member_id", "name"];
  if (isUndashedUuid(text)) return ["uuid_undashed"];
  if (isDashedUuid(text)) return ["uuid_dashed"];
  if (GUILD_ID.test(text)) return ["guild_id"];
  if (KEY_PREFIX.test(text)) return ["key_prefix", "name"];
  if (IPV4.test(text)) return ["ipv4"];
  if (IPV6.test(text)) return ["ipv6"];
  if (NAME.test(text)) return ["name"];
  if (SHORT_NAME.test(text)) return ["short_name"];
  return ["text"];
}

function parsePastedUrl(text: string, adminOrigin: string): PastedUrl | null {
  if (!URL.canParse(text)) return null;
  const url = new URL(text);
  if (url.origin === adminOrigin) return { kind: "admin", path: `${url.pathname}${url.search}${url.hash}` };
  const fromParams = IDENTIFIER_PARAMS.flatMap((param) => url.searchParams.getAll(param)).flatMap((value) => value.split(","));
  const segments = url.pathname.split("/").map(decodeSegment);
  const fromPath = segments.filter((segment, index) => isUuid(segment) || (NAME.test(segment) && NAME_SEGMENT_PARENTS.has(segments[index - 1] ?? "")));
  const identifiers = [...new Set([...fromParams, ...fromPath].map((value) => normalizeUuid(value) ?? value.trim()).filter((value) => value !== ""))];
  return { kind: "api", identifiers, carriesKey: url.searchParams.has("key") };
}

function decodeSegment(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

function apiUrlLabel(url: Extract<PastedUrl, { kind: "api" }>): string {
  const count = url.identifiers.length;
  const identifiers = `${count} identifier${count === 1 ? "" : "s"}`;
  return url.carriesKey ? `API URL · ${identifiers} · carries a key` : `API URL · ${identifiers}`;
}
