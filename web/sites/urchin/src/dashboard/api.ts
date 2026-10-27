import { account } from "@shared/account";

const base = document.querySelector<HTMLElement>(".dashboard")!.dataset.api!;

export const me = account(base);

export function send(path: string, method = "GET", body?: unknown) {
  return fetch(`${base}${path}`, {
    method,
    credentials: "include",
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  }).then(
    (response) => (response.ok ? response : undefined),
    () => undefined,
  );
}

export async function load<T>(
  path: string,
  method = "GET",
): Promise<T | undefined> {
  return (await send(path, method))?.json();
}
