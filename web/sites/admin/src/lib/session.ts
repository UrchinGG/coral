export interface Session {
  discord_id: string;
  build: string | null;
}

const fromPage = () => {
  const inlined = document.getElementById("boot")?.textContent;
  return inlined ? (JSON.parse(inlined) as Session) : undefined;
};

export const session = async (): Promise<Session | undefined> =>
  fromPage() ??
  fetch("/api/session").then(
    (response) => (response.ok ? response.json() : undefined),
    () => undefined,
  );
