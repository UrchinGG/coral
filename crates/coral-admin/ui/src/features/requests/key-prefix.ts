export const KEY_PREFIX_CHARS = 8;

export function keyPrefixOf(text: string): string {
  return text.slice(0, KEY_PREFIX_CHARS);
}
