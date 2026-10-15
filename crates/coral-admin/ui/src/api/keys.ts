import type { BlacklistFilters, GuildSort, StrikeFilter } from "./types/platform";
import type { GroupQuery, RequestQuery } from "./types/request-explorer";
import type { TrafficRange } from "./types/traffic";

export const keys = {
  session: () => ["session"] as const,
  search: {
    all: () => ["search"] as const,
    query: (q: string) => ["search", "query", q] as const,
    remote: (name: string) => ["search", "remote", name] as const,
  },
  accounts: {
    all: () => ["accounts"] as const,
    core: (discordId: string) => ["accounts", "core", discordId] as const,
  },
  players: {
    all: () => ["players"] as const,
    core: (uuid: string) => ["players", "core", uuid] as const,
    of: (uuid: string) => ["players", "of", uuid] as const,
    lists: () => ["players", "lists"] as const,
  },
  requests: {
    all: () => ["requests"] as const,
    list: (query: RequestQuery) => ["requests", "list", query] as const,
    groups: (query: GroupQuery) => ["requests", "groups", query] as const,
    histogram: (series: string) => ["requests", "histogram", series] as const,
    routes: () => ["requests", "routes"] as const,
    detail: (requestId: string, ts: string) => ["requests", "detail", requestId, ts] as const,
  },
  ips: {
    all: () => ["ips"] as const,
    core: (ip: string) => ["ips", "core", ip] as const,
  },
  apiKeys: {
    all: () => ["keys"] as const,
    byPrefix: (prefix: string) => ["keys", "prefix", prefix] as const,
    byId: (keyId: number) => ["keys", "id", keyId] as const,
  },
  guilds: {
    all: () => ["guilds"] as const,
    core: (guildId: string) => ["guilds", "core", guildId] as const,
    list: (q: string, sort: GuildSort) => ["guilds", "list", q, sort] as const,
    at: (guildId: string, ts: string) => ["guilds", "at", guildId, ts] as const,
  },
  plugins: {
    all: () => ["plugins"] as const,
    core: (slug: string) => ["plugins", "core", slug] as const,
    list: (q: string) => ["plugins", "list", q] as const,
  },
  moderation: {
    all: () => ["moderation"] as const,
    recent: () => ["moderation", "recent"] as const,
    tagTypes: () => ["moderation", "tag-types"] as const,
    blacklist: (filters: BlacklistFilters) => ["moderation", "blacklist", filters] as const,
    strikes: (state: StrikeFilter) => ["moderation", "strikes", state] as const,
    audit: (action: string) => ["moderation", "audit", action] as const,
    guide: () => ["moderation", "guide"] as const,
    rules: () => ["moderation", "rules"] as const,
  },
  server: {
    all: () => ["server"] as const,
    config: () => ["server", "config"] as const,
    directory: () => ["server", "directory"] as const,
    population: () => ["server", "population"] as const,
    jobs: () => ["server", "jobs"] as const,
  },
  starfish: {
    all: () => ["starfish"] as const,
    users: (q: string) => ["starfish", "users", q] as const,
  },
  maintenance: () => ["maintenance"] as const,
  extensions: {
    all: () => ["extensions"] as const,
    view: (id: string) => ["extensions", "view", id] as const,
  },
  traffic: {
    all: () => ["traffic"] as const,
    summary: (range: TrafficRange) => ["traffic", "summary", range] as const,
  },
  flags: () => ["flags"] as const,
};
