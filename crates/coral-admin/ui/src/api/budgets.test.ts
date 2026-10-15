import { describe, expect, it } from "vitest";

import { budgetFor, DEFAULT_QUERY_BUDGET_MS } from "./budgets";

describe("budgetFor", () => {
  it.each([
    ["GET /api/session", 20],
    ["POST /api/search", 120],
    ["POST /api/search/remote", 400],
    ["POST /api/labels", 15],
    ["POST /api/requests/query", 60],
    ["GET /api/requests/0199e3f12c3a7b4d8e5f6a7b8c9d0e1f?t=2026-10-14T14%3A32%3A10.412318Z", 25],
    ["GET /api/requests/94589c87999b4a7ab3add2ae50711e64", 60],
    ["GET /api/traffic/summary?window=24h", 40],
    ["GET /api/traffic/series?window=7d", 30],
    ["GET /api/traffic/pressure", 25],
    ["GET /api/players/top", 20],
    ["GET /api/players/94589c87999b4a7ab3add2ae50711e64", 80],
    ["GET /api/players/94589c87999b4a7ab3add2ae50711e64/requesters", 80],
    ["GET /api/accounts/1218840276405911602/lookups/summary", 10],
    ["GET /api/accounts/1218840276405911602/lookups/top", 150],
    ["GET /api/accounts/1218840276405911602/lookups?cursor=abc", 30],
    ["GET /api/ips/198.51.100.23", 40],
  ])("gives %s its spec budget", (endpoint, ms) => {
    expect(budgetFor(endpoint)).toBe(ms);
  });

  it("falls back to the default for endpoints the table does not list", () => {
    expect(budgetFor("GET /api/accounts/1218840276405911602")).toBe(DEFAULT_QUERY_BUDGET_MS);
    expect(budgetFor("GET /api/members?page=2")).toBe(DEFAULT_QUERY_BUDGET_MS);
  });

  it("matches the method as well as the path", () => {
    expect(budgetFor("GET /api/search")).toBe(DEFAULT_QUERY_BUDGET_MS);
  });
});
