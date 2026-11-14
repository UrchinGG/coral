import { el } from "@/lib/dom";
import type { Labels } from "@/lib/members";

export type Ref =
  | { kind: "account"; discord_id: string; member_id: number | null }
  | { kind: "player"; uuid: string }
  | { kind: "name"; name: string }
  | { kind: "ip"; ip: string; country: string | null }
  | { kind: "key"; prefix: string; key_kind: string }
  | { kind: "guild"; guild_id: string; name: string | null }
  | { kind: "plugin"; slug: string; name: string | null }
  | { kind: "internal"; service: string }
  | { kind: "request"; request_id: string; ts: string }
  | { kind: "anonymous" };

export function refName(ref: Ref | null, labels: Labels): string {
  if (!ref) return "";
  switch (ref.kind) {
    case "account":
      return labels.discord[ref.discord_id] ?? ref.discord_id;
    case "player":
      return labels.minecraft[ref.uuid] ?? ref.uuid.slice(0, 8);
    case "name":
      return ref.name;
    case "ip":
      return ref.ip;
    case "key":
      return `${ref.prefix}… (${ref.key_kind})`;
    case "guild":
      return ref.name ?? ref.guild_id;
    case "plugin":
      return ref.name ?? ref.slug;
    case "internal":
      return `Internal ${ref.service}`;
    case "request":
      return ref.request_id.slice(0, 8);
    case "anonymous":
      return "Anonymous";
  }
}

export function refUrl(ref: Ref | null): string | null {
  if (!ref) return null;
  switch (ref.kind) {
    case "account":
      return `/urchin/members?id=${ref.discord_id}`;
    case "player":
      return `/urchin/players?uuid=${ref.uuid}`;
    case "name":
      return `/urchin/players?q=${encodeURIComponent(ref.name)}`;
    case "guild":
      return `/urchin/guilds?id=${ref.guild_id}`;
    case "plugin":
      return `/starfish/plugins?slug=${ref.slug}`;
    case "ip":
      return `/platform/traffic?ip=${encodeURIComponent(ref.ip)}`;
    default:
      return null;
  }
}

export function refLink(ref: Ref | null, labels: Labels) {
  const url = refUrl(ref);
  const text = refName(ref, labels);
  return url
    ? el("a", {
        href: url,
        className: `ref ref-${ref!.kind}`,
        textContent: text,
      })
    : el("span", { textContent: text });
}
