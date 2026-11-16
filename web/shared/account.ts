export interface Account {
  id: string;
  name: string;
  username: string;
  avatar: string;
  rank: string | null;
  starfish_licensed: boolean;
}

export const SIGN_IN = "https://urchin.gg/auth/discord";

let current: Promise<Account | undefined> | undefined;

export const account = () =>
  (current ??= fetch("/auth/me").then(
    (response) => (response.ok ? response.json() : undefined),
    () => undefined,
  ));

export const signInUrl = () =>
  `${SIGN_IN}?return=${encodeURIComponent(location.href)}`;

export const signOut = async () => {
  await fetch("/auth/logout", { method: "POST" });
  location.reload();
};
