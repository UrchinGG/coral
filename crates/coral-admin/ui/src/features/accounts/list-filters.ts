import type { AccountListFilters, AccountSort, SortDirection } from "../../api/types/accounts";
import { pickParam } from "./params";
import { ACCESS_LEVELS } from "./vocabulary";

export type Choice = "any" | "yes" | "no";
export type FlagFilter = "locked" | "has_key" | "has_dev_key" | "starfish";

export const ANY: Choice = "any";
export const SORTS: readonly { value: AccountSort; label: string }[] = [
  { value: "member", label: "Member number" },
  { value: "joined", label: "Joined" },
  { value: "requests", label: "Lifetime requests" },
  { value: "access", label: "Access level" },
];
export const SORT_VALUES: readonly AccountSort[] = SORTS.map((option) => option.value);
export const DEFAULT_SORT: AccountSort = "member";
export const DIRECTIONS: readonly SortDirection[] = ["desc", "asc"];
export const DEFAULT_DIRECTION: SortDirection = "desc";
export const CHOICES: readonly Choice[] = [ANY, "yes", "no"];
export const RANKS: readonly string[] = [ANY, ...ACCESS_LEVELS.filter((rank) => rank.level > 0).map((rank) => String(rank.level))];
export const FLAGS: readonly FlagFilter[] = ["locked", "has_key", "has_dev_key", "starfish"];

export function listFiltersFrom(search: URLSearchParams): AccountListFilters {
  const rank = pickParam(search, "rank", RANKS, ANY);
  const flag = (name: FlagFilter) => choiceValue(pickParam(search, name, CHOICES, ANY));
  return {
    sort: pickParam(search, "sort", SORT_VALUES, DEFAULT_SORT),
    dir: pickParam(search, "dir", DIRECTIONS, DEFAULT_DIRECTION),
    rank: rank === ANY ? null : Number(rank),
    locked: flag("locked"),
    has_key: flag("has_key"),
    has_dev_key: flag("has_dev_key"),
    starfish: flag("starfish"),
  };
}

function choiceValue(choice: Choice): boolean | null {
  return choice === ANY ? null : choice === "yes";
}
