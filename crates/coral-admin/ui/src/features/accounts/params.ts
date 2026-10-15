export function pickParam<T extends string>(search: URLSearchParams, name: string, values: readonly T[], fallback: T): T {
  return values.find((candidate) => candidate === search.get(name)) ?? fallback;
}
