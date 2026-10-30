import type { ReplayPlayer } from "../viewer/pkg/replay_viewer.js";

export interface Nick {
  realName: string | null;
}

const NPC_NAME = /^(?=.*\d)[a-z0-9]{10}$/;
const TRUNCATED_NAME_LENGTH = 14;

const withoutLeadingZeros = (hash: string) => hash.replace(/^0+/, "");

export const nickSkins = fetch("/replay-assets/nick-skins.json")
  .then((response) => response.json())
  .then((hashes: string[]) => new Set(hashes.map(withoutLeadingZeros)))
  .catch((error) => {
    console.warn(error);
    return new Set<string>();
  });

const nickOf = (
  username: string,
  { skinHash, skinOwner }: ReplayPlayer,
  skins: Set<string>,
): Nick | undefined => {
  if (NPC_NAME.test(username)) return;
  if (skinHash && skins.has(withoutLeadingZeros(skinHash))) {
    return { realName: null };
  }
  if (skinOwner && skinOwner.toLowerCase() !== username.toLowerCase()) {
    return { realName: skinOwner };
  }
};

export const denick = (player: ReplayPlayer, skins: Set<string>) => {
  const { username, skinOwner } = player;
  const fullName =
    username.length >= TRUNCATED_NAME_LENGTH && skinOwner?.startsWith(username)
      ? skinOwner
      : username;
  return { username: fullName, nick: nickOf(fullName, player, skins) };
};

export const nickTag = ({ nick }: { nick?: Nick }) =>
  nick ? (nick.realName ? ` (${nick.realName})` : " [NICK]") : "";
