export type KeyKind = "none" | "personal" | "developer" | "internal" | "session" | "invalid" | "unverified" | "legacy" | "minecraft";

export type Cause =
  | "ok"
  | "caller_limit"
  | "upstream_limit"
  | "upstream_error"
  | "auth"
  | "validation"
  | "not_found"
  | "internal"
  | "unmatched";

export type Service = "unknown" | "bot" | "sync" | "verify";

export type AccountRef = { kind: "account"; discord_id: string; member_id: number | null };
export type PlayerRef = { kind: "player"; uuid: string };
export type NameRef = { kind: "name"; name: string };
export type RequestRef = { kind: "request"; request_id: string; ts: string };
export type IpRef = { kind: "ip"; ip: string; country: string | null; proxied: boolean };
export type KeyRef = { kind: "key"; key_id: number | null; prefix: string; key_kind: KeyKind };
export type GuildRef = { kind: "guild"; guild_id: string; name: string | null };
export type PluginRef = { kind: "plugin"; slug: string; name: string | null };
export type InternalRef = { kind: "internal"; service: Service };
export type AnonymousRef = { kind: "anonymous" };

export type EntityRef =
  | AccountRef
  | PlayerRef
  | NameRef
  | RequestRef
  | IpRef
  | KeyRef
  | GuildRef
  | PluginRef
  | InternalRef
  | AnonymousRef;

export type EntityKind = EntityRef["kind"];

export type Labels = {
  discord: Record<string, string>;
  minecraft: Record<string, string>;
  pending: { discord: string[]; minecraft: string[] };
};

export type Page<T> = {
  items: T[];
  next_cursor: string | null;
  scanned_to: string | null;
  clamped_from: string | null;
  labels: Labels;
};

export type SubjectForm = "undashed" | "dashed" | "name" | "display" | "path" | "body";

export type Subject = {
  player: string | null;
  name: string | null;
  raw: string;
  form: SubjectForm;
};
