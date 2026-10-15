const BY_ID = "id/";

export type KeyRoute = { kind: "id"; keyId: number } | { kind: "prefix"; prefix: string };

export function keyRouteOf(rest: string): KeyRoute {
  return rest.startsWith(BY_ID) ? { kind: "id", keyId: Number(rest.slice(BY_ID.length)) } : { kind: "prefix", prefix: rest };
}
