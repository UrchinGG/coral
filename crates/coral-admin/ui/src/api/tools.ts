import { postJson } from "./http";
import { withLabels } from "./labels";
import type { Resolved } from "./types/platform";

export const MAX_RESOLVE_INPUTS = 200;

export function resolveIdentifiers(inputs: readonly string[]): Promise<Resolved> {
  return postJson<Resolved>("/tools/resolve", { inputs }).then(withLabels);
}
