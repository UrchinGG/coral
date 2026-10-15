import type { EntityKind, EntityRef, Service } from "../api/types/refs";

export type LabelSpace = "discord" | "minecraft";

export type LabelTarget = { space: LabelSpace; id: string };

const SNOWFLAKE = /^\d{17,20}$/;

export const ENTITY_KIND_LABEL: Record<EntityKind, string> = {
  account: "Account",
  player: "Player",
  name: "Name",
  request: "Request",
  ip: "IP",
  key: "Key",
  guild: "Guild",
  plugin: "Plugin",
  internal: "Internal",
  anonymous: "Anonymous",
};

export const SERVICE_LABEL: Record<Service, string> = {
  unknown: "internal (unknown service)",
  bot: "coral-bot",
  sync: "coral-sync",
  verify: "mc-verify",
};

export function isSnowflake(text: string): boolean {
  return SNOWFLAKE.test(text);
}

export function refHref(ref: EntityRef): string | null {
  switch (ref.kind) {
    case "account":
      return `/accounts/${ref.discord_id}`;
    case "player":
      return `/players/${ref.uuid}`;
    case "name":
      return `/requests?${new URLSearchParams({ player: ref.name, include_display: "1" })}`;
    case "request":
      return `/requests/${ref.request_id}?${new URLSearchParams({ t: ref.ts })}`;
    case "ip":
      return `/ips/${encodeURIComponent(ref.ip)}`;
    case "key":
      return ref.key_id === null ? `/keys/${ref.prefix}` : `/keys/id/${ref.key_id}`;
    case "guild":
      return `/guilds/${ref.guild_id}`;
    case "plugin":
      return `/plugins/${encodeURIComponent(ref.slug)}`;
    case "internal":
    case "anonymous":
      return null;
  }
}

export function refIdentity(ref: EntityRef): string {
  return `${ref.kind}:${refId(ref)}`;
}

export function refId(ref: EntityRef): string {
  switch (ref.kind) {
    case "account":
      return ref.discord_id;
    case "player":
      return ref.uuid;
    case "name":
      return ref.name;
    case "request":
      return ref.request_id;
    case "ip":
      return ref.ip;
    case "key":
      return ref.key_id === null ? ref.prefix : String(ref.key_id);
    case "guild":
      return ref.guild_id;
    case "plugin":
      return ref.slug;
    case "internal":
      return ref.service;
    case "anonymous":
      return "anonymous";
  }
}

export function refFallbackText(ref: EntityRef): { text: string; mono: boolean } {
  switch (ref.kind) {
    case "name":
      return { text: ref.name, mono: false };
    case "key":
      return { text: ref.prefix, mono: true };
    case "guild":
      return ref.name === null ? { text: ref.guild_id, mono: true } : { text: ref.name, mono: false };
    case "plugin":
      return { text: ref.name ?? ref.slug, mono: ref.name === null };
    case "internal":
      return { text: SERVICE_LABEL[ref.service], mono: false };
    case "anonymous":
      return { text: "anonymous", mono: false };
    default:
      return { text: refId(ref), mono: true };
  }
}

export function labelTargetOf(ref: EntityRef): LabelTarget | null {
  if (ref.kind === "account") return { space: "discord", id: ref.discord_id };
  if (ref.kind === "player") return { space: "minecraft", id: ref.uuid };
  return null;
}
