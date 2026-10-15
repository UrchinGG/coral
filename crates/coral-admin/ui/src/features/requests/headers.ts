export type HeaderGroup = "credentials" | "client" | "cloudflare" | "proxy" | "other";

export type GroupedHeaders = { group: HeaderGroup; headers: [string, string][] }[];

export const HEADER_GROUP_LABEL: Record<HeaderGroup, string> = {
  credentials: "Credentials",
  client: "Client",
  cloudflare: "Cloudflare",
  proxy: "Proxy",
  other: "Other",
};

const GROUP_ORDER: readonly HeaderGroup[] = ["credentials", "client", "cloudflare", "proxy", "other"];
const CREDENTIAL_HEADERS = new Set(["x-api-key", "authorization", "proxy-authorization", "cookie", "x-starfish-session"]);
const CLIENT_HEADERS = new Set(["user-agent", "host", "origin", "referer", "connection", "cache-control", "pragma", "priority", "dnt", "te", "upgrade-insecure-requests"]);
const CLIENT_PREFIXES = ["accept", "content-", "sec-", "x-coral-"] as const;
const CLOUDFLARE_HEADERS = new Set(["cdn-loop", "true-client-ip"]);
const CLOUDFLARE_PREFIXES = ["cf-"] as const;
const PROXY_HEADERS = new Set(["forwarded", "via", "x-real-ip"]);
const PROXY_PREFIXES = ["x-forwarded-"] as const;

export function groupHeaders(headers: Record<string, string>): GroupedHeaders {
  const sorted = Object.entries(headers).sort(([a], [b]) => a.localeCompare(b));
  return GROUP_ORDER.map((group) => ({ group, headers: sorted.filter(([name]) => headerGroup(name) === group) })).filter((entry) => entry.headers.length > 0);
}

export function headerGroup(name: string): HeaderGroup {
  const lower = name.toLowerCase();
  if (CREDENTIAL_HEADERS.has(lower)) return "credentials";
  if (CLOUDFLARE_HEADERS.has(lower) || startsWithAny(lower, CLOUDFLARE_PREFIXES)) return "cloudflare";
  if (PROXY_HEADERS.has(lower) || startsWithAny(lower, PROXY_PREFIXES)) return "proxy";
  if (CLIENT_HEADERS.has(lower) || startsWithAny(lower, CLIENT_PREFIXES)) return "client";
  return "other";
}

export function rawHeaders(headers: Record<string, string>): string {
  return Object.entries(headers)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([name, value]) => `${name}: ${value}`)
    .join("\n");
}

function startsWithAny(name: string, prefixes: readonly string[]): boolean {
  return prefixes.some((prefix) => name.startsWith(prefix));
}
