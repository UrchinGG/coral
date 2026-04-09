const API_URL = process.env.CORAL_API_URL || "http://localhost:8000";
const API_KEY = process.env.CORAL_INTERNAL_KEY || "";

type RequestInit = globalThis.RequestInit;

async function get<T>(path: string, init?: RequestInit): Promise<T | null> {
  try {
    const res = await fetch(`${API_URL}${path}`, {
      ...init,
      headers: { "X-API-Key": API_KEY, ...init?.headers },
      signal: init?.signal ?? AbortSignal.timeout(10_000),
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}


export type ResolveResult = { uuid: string; username: string };

export async function resolve(identifier: string): Promise<ResolveResult | null> {
  return get<ResolveResult>(`/v3/resolve/${encodeURIComponent(identifier)}`);
}


export type PlayerStats = {
  uuid: string;
  username: string;
  hypixel?: Record<string, unknown>;
  tags?: PlayerTags["tags"];
};

export async function getPlayer(uuid: string): Promise<PlayerStats | null> {
  return get<PlayerStats>(`/v3/player/profile?uuid=${encodeURIComponent(uuid)}`);
}


export type PlayerTags = {
  uuid: string;
  tags: Array<{
    tag_type: string;
    reason: string;
    added_by: number;
    added_on: number;
    hide_username: boolean;
  }>;
};

export async function getTags(uuid: string): Promise<PlayerTags | null> {
  return get<PlayerTags>(`/v3/player/tags?uuid=${encodeURIComponent(uuid)}`);
}
