import { send } from "@/lib/api";
import type { Labels } from "@/lib/members";
import type { Ref } from "@/lib/refs";
import { API } from "@/lib/paths";

export interface SearchHit {
  ref: Ref;
  score: number;
  exact: boolean;
  reason: string;
}

export interface SearchResponse {
  hits: SearchHit[];
  remote_pending: boolean;
  labels: Labels;
}

const SEARCH_LIMIT = 20;

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${API}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`Search answered ${response.status}`);
  return response.json();
}

export const search = (q: string) =>
  post<SearchResponse>("/search", { q, limit: SEARCH_LIMIT });

export const searchMojang = (name: string) =>
  post<SearchHit[]>("/search/remote", { name });

export { send };
