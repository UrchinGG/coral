export interface Account {
  id: string;
  name: string;
  avatar: string;
  rank: string | null;
  starfish_licensed: boolean;
}

let current: Promise<Account | undefined> | undefined;

export const account = (api: string) =>
  (current ??= fetch(`${api}/auth/me`, { credentials: "include" }).then(
    (response) => (response.ok ? response.json() : undefined),
    () => undefined,
  ));

export const signInUrl = (api: string) =>
  `${api}/auth/discord?return=${encodeURIComponent(location.href)}`;

export const signOut = async (api: string) => {
  await fetch(`${api}/auth/logout`, { method: "POST", credentials: "include" });
  location.reload();
};
