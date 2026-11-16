import { API } from "@/lib/paths";

export type Permission =
  | "signed_in"
  | "staff"
  | "players_read"
  | "tags_write"
  | "members_read"
  | "tagging_write"
  | "strikes_write"
  | "access_level_write"
  | "player_locks_write"
  | "sign_in_blocks_write"
  | "key_limits_write"
  | "reviews_manage"
  | "keys_manage"
  | "traffic"
  | "starfish"
  | "owner";

export type Rank = "Default" | "Helper" | "Moderator" | "Admin" | "Owner";

export interface Session {
  discord_id: string;
  rank: Rank;
  permissions: Permission[];
  build: string | null;
}

const RANK_ORDER: Rank[] = ["Default", "Helper", "Moderator", "Admin", "Owner"];

const fromPage = () => {
  const inlined = document.getElementById("boot")?.textContent;
  return inlined ? (JSON.parse(inlined) as Session) : undefined;
};

let current: Promise<Session | undefined> | undefined;

export const session = (): Promise<Session | undefined> =>
  (current ??= Promise.resolve(
    fromPage() ??
      fetch(`${API}/session`).then(
        (response) => (response.ok ? response.json() : undefined),
        () => undefined,
      ),
  ));

export const can = (viewer: Session | undefined, permission: Permission) =>
  viewer?.permissions.includes(permission) ?? false;

export const outranks = (viewer: Session | undefined, target: Rank) =>
  viewer !== undefined &&
  (viewer.rank === "Owner" ||
    RANK_ORDER.indexOf(viewer.rank) > RANK_ORDER.indexOf(target));

export function rankOf(level: number, isOwner = false): Rank {
  if (isOwner) return "Owner";
  return (
    RANK_ORDER[Math.max(0, Math.min(level, RANK_ORDER.length) - 1)] ?? "Default"
  );
}

export const BELOW_YOU = "Only for members ranked below you";
