const UNDASHED_UUID = /^[0-9a-f]{32}$/i;
const DASHED_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const UUID_V7 = 7;
const VERSION_NIBBLE_INDEX = 12;
const TIMESTAMP_HEX_LENGTH = 12;
const DASH_POSITIONS = [8, 12, 16, 20] as const;

export function isUndashedUuid(text: string): boolean {
  return UNDASHED_UUID.test(text);
}

export function isDashedUuid(text: string): boolean {
  return DASHED_UUID.test(text);
}

export function isUuid(text: string): boolean {
  return isUndashedUuid(text) || isDashedUuid(text);
}

export function normalizeUuid(text: string): string | null {
  return isUuid(text) ? text.replaceAll("-", "").toLowerCase() : null;
}

export function dashUuid(undashed: string): string {
  const [a, b, c, d] = DASH_POSITIONS;
  return [undashed.slice(0, a), undashed.slice(a, b), undashed.slice(b, c), undashed.slice(c, d), undashed.slice(d)].join("-");
}

export function uuidVersion(undashed: string): number {
  return Number.parseInt(undashed[VERSION_NIBBLE_INDEX], 16);
}

export function isUuidV7(undashed: string): boolean {
  return uuidVersion(undashed) === UUID_V7;
}

export function uuidV7Timestamp(undashed: string): string | null {
  if (!isUuidV7(undashed)) return null;
  const millis = Number.parseInt(undashed.slice(0, TIMESTAMP_HEX_LENGTH), 16);
  return new Date(millis).toISOString();
}
