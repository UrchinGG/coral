import { API } from "@/lib/paths";

const STATUS_TEXT: Record<number, string> = {
  401: "Your sign-in ran out. Reload the page to sign in again.",
  403: "You don't have access to this.",
  404: "This wasn't found.",
  429: "Too many requests just now. Try again in a moment.",
};

export async function load<T>(path: string): Promise<T> {
  const response = await fetch(`${API}${path}`).catch(() => null);
  if (!response)
    throw new Error("Couldn't reach the server. Check your connection.");
  if (!response.ok) throw new Error(failure(response.status));
  return response.json();
}

/** failure says what a failed answer means in words, keeping the status for anyone reporting it. */
export const failure = (status: number) =>
  STATUS_TEXT[status] ?? `The server had a problem (${status}). Try again.`;

export async function send(path: string, method: string, body?: unknown) {
  const response = await fetch(`${API}${path}`, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  return response.ok;
}

export async function call<T>(
  path: string,
  method: string,
  body?: unknown,
): Promise<T> {
  const response = await fetch(`${API}${path}`, {
    method,
    headers:
      body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  const answer = text ? JSON.parse(text) : null;
  if (!response.ok)
    throw new Error(answer?.error?.message ?? failure(response.status));
  return answer as T;
}
