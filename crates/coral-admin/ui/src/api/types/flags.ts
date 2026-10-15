import type { FlagLevel } from "./extension";
import type { EntityRef } from "./refs";

export type FlagKind = "upstream" | "writer" | "pressure" | "probe" | "spike" | "extension";

export type Flag = {
  key: string;
  kind: FlagKind;
  level: FlagLevel;
  summary: string;
  refs: EntityRef[];
  extension: string | null;
};

export type FlagFailure = { kind: FlagKind; message: string };

export type Flags = { generated_at: string; flags: Flag[]; failures: FlagFailure[] };

export type Dismissal = { key: string; dismissed_until: string };
