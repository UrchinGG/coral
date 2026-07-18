import { useQuery } from "@tanstack/react-query";
import { apiGet } from "./client";
import type {
  Bucket,
  BudgetRow,
  PathCount,
  RateLimits,
  RequestListResponse,
  Stats,
} from "./types";

const REFRESH_MS = 15_000;

export function useStats(hours: number) {
  return useQuery({
    queryKey: ["requests", "stats", hours],
    queryFn: () => apiGet<Stats>(`/requests/stats?hours=${hours}`),
    refetchInterval: REFRESH_MS,
  });
}

export type SeriesMode = "incoming" | "endpoint" | "hypixel";

export function useSeries(mode: SeriesMode, hours: number, path: string) {
  return useQuery({
    queryKey: ["requests", "series", mode, hours, path],
    queryFn: () => {
      if (mode === "hypixel") {
        return apiGet<Bucket[]>(`/requests/hypixel-series?hours=${hours}`);
      }
      const pathParam = mode === "endpoint" && path ? `&path=${encodeURIComponent(path)}` : "";
      return apiGet<Bucket[]>(`/requests/series?hours=${hours}${pathParam}`);
    },
    refetchInterval: REFRESH_MS,
  });
}

export function usePaths(hours: number) {
  return useQuery({
    queryKey: ["requests", "paths", hours],
    queryFn: () => apiGet<PathCount[]>(`/requests/paths?hours=${hours}`),
  });
}

export function useRateLimits() {
  return useQuery({
    queryKey: ["requests", "ratelimits"],
    queryFn: () => apiGet<RateLimits>("/requests/ratelimits"),
    refetchInterval: REFRESH_MS,
  });
}

export function useBudgets() {
  return useQuery({
    queryKey: ["requests", "budgets"],
    queryFn: () => apiGet<BudgetRow[]>("/requests/budgets"),
    refetchInterval: REFRESH_MS,
  });
}

export type LogFilters = {
  hours: number;
  path?: string;
  path_exact?: boolean;
  method?: string;
  status?: string;
  key_prefix?: string;
  ip?: string;
  discord_id?: string;
  caller?: string;
  error_contains?: string;
  errors?: boolean;
  q?: string;
  from?: number;
  to?: number;
};

export function logFiltersToParams(filters: LogFilters, offset: number, limit: number): URLSearchParams {
  const params = new URLSearchParams();
  params.set("hours", String(filters.hours));
  params.set("offset", String(offset));
  params.set("limit", String(limit));
  if (filters.path) params.set("path", filters.path);
  if (filters.path_exact) params.set("path_exact", "true");
  if (filters.method) params.set("method", filters.method);
  if (filters.status) params.set("status", filters.status);
  if (filters.key_prefix) params.set("key_prefix", filters.key_prefix);
  if (filters.ip) params.set("ip", filters.ip);
  if (filters.discord_id) params.set("discord_id", filters.discord_id);
  if (filters.caller) params.set("caller", filters.caller);
  if (filters.error_contains) params.set("error_contains", filters.error_contains);
  if (filters.errors) params.set("errors", "true");
  if (filters.q) params.set("q", filters.q);
  if (filters.from) params.set("from", String(filters.from));
  if (filters.to) params.set("to", String(filters.to));
  return params;
}

export function useRequestLog(filters: LogFilters, offset: number, limit: number) {
  const params = logFiltersToParams(filters, offset, limit);

  return useQuery({
    queryKey: ["requests", "log", params.toString()],
    queryFn: () => apiGet<RequestListResponse>(`/requests?${params}`),
    refetchInterval: REFRESH_MS,
  });
}
