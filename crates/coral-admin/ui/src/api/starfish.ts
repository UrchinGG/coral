import { keys } from "./keys";
import { rowPages } from "./row-pages";
import type { StarfishUser } from "./types/platform";

export function starfishUsersQuery(q: string) {
  const params = new URLSearchParams();
  if (q !== "") params.set("q", q);
  return rowPages<StarfishUser>(keys.starfish.users(q), "/starfish/users", params);
}
