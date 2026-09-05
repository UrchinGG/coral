const STARFISH_HOSTS = ["starfish.urchin.gg", "starfish.hexze.dev", "localhost:3000"];

/** Returns the request's origin if it's one of the recognized Starfish hosts, else null. */
export function starfishOrigin(request: Request): string | null {
  const host = request.headers.get("host") ?? "";
  if (!STARFISH_HOSTS.includes(host)) return null;
  const protocol = host.startsWith("localhost") ? "http" : "https";
  return `${protocol}://${host}`;
}
