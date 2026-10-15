import type { PlayerCore } from "../../api/types/players";

export function hasProfile(core: PlayerCore): boolean {
  return core.snapshots.count > 0 || core.names.length > 0 || core.owners.length > 0 || core.blacklist.active.length > 0;
}

export function currentName(core: PlayerCore | undefined): string | null {
  return core?.names.find((name) => name.current)?.name ?? null;
}

export function formerNames(core: PlayerCore | undefined): string[] {
  return core?.names.filter((name) => !name.current).map((name) => name.name) ?? [];
}
