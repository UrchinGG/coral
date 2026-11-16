import { el } from "@/lib/dom";

export interface SyncServer {
  id: string;
  name: string;
  icon_url: string | null;
  enabled: boolean;
  members: number | null;
}

export function serverIcon(
  server: { name: string; icon_url: string | null },
  className = "server-icon",
) {
  if (server.icon_url)
    return el("img", { className, src: server.icon_url, alt: "" });
  return el(
    "span",
    { className, ariaHidden: "true" },
    server.name.trim().charAt(0).toUpperCase(),
  );
}
