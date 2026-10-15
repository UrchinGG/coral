type EndpointBudget = { method: string; path: RegExp; ms: number; whenParam?: string };

export const DEFAULT_QUERY_BUDGET_MS = 60;

const QUERY_SEPARATOR = "?";
const ENDPOINT_BUDGETS: readonly EndpointBudget[] = [
  { method: "GET", path: /^\/api\/session$/, ms: 20 },
  { method: "POST", path: /^\/api\/search$/, ms: 120 },
  { method: "POST", path: /^\/api\/search\/remote$/, ms: 400 },
  { method: "POST", path: /^\/api\/labels$/, ms: 15 },
  { method: "POST", path: /^\/api\/requests\/(?:query|groups)$/, ms: 60 },
  { method: "GET", path: /^\/api\/requests\/[^/]+$/, ms: 25, whenParam: "t" },
  { method: "GET", path: /^\/api\/requests\/[^/]+$/, ms: 60 },
  { method: "GET", path: /^\/api\/traffic\/summary$/, ms: 40 },
  { method: "GET", path: /^\/api\/traffic\/series$/, ms: 30 },
  { method: "GET", path: /^\/api\/traffic\/(?:routes|callers)$/, ms: 60 },
  { method: "GET", path: /^\/api\/traffic\/(?:pressure|upstream)$/, ms: 25 },
  { method: "GET", path: /^\/api\/flags$/, ms: 60 },
  { method: "GET", path: /^\/api\/players\/top$/, ms: 20 },
  { method: "GET", path: /^\/api\/players\/[^/]+(?:\/requests|\/requesters)?$/, ms: 80 },
  { method: "GET", path: /^\/api\/accounts\/[^/]+\/lookups\/summary$/, ms: 10 },
  { method: "GET", path: /^\/api\/accounts\/[^/]+\/lookups\/top$/, ms: 150 },
  { method: "GET", path: /^\/api\/accounts\/[^/]+\/lookups$/, ms: 30 },
  { method: "GET", path: /^\/api\/ips\/[^/]+$/, ms: 40 },
  { method: "GET", path: /^\/api\/extensions\/[^/]+$/, ms: 50 },
];

export function budgetFor(endpoint: string): number {
  const [method, target = ""] = endpoint.split(" ", 2);
  const [path, query = ""] = target.split(QUERY_SEPARATOR, 2);
  const params = new URLSearchParams(query);
  const budget = ENDPOINT_BUDGETS.find((candidate) => candidate.method === method && candidate.path.test(path) && (candidate.whenParam === undefined || params.has(candidate.whenParam)));
  return budget?.ms ?? DEFAULT_QUERY_BUDGET_MS;
}
