import type { Metadata } from "next";
import { headers } from "next/headers";
import { notFound } from "next/navigation";

const DESCRIPTION = "Hypixel proxy with game overlay and Lua scripting.";

export const metadata: Metadata = {
  // `absolute` (not `default`) so this doesn't get composed with the root
  // layout's "%s — Coral" template — the Starfish section has its own identity.
  title: { absolute: "Starfish", template: "%s — Starfish" },
  description: DESCRIPTION,
  keywords: ["Starfish", "Hypixel", "Minecraft", "Lua"],
  openGraph: {
    type: "website",
    siteName: "Starfish",
    title: "Starfish",
    description: DESCRIPTION,
    images: [],
  },
  twitter: {
    card: "summary",
    title: "Starfish",
    description: DESCRIPTION,
    images: [],
  },
};

export default async function StarfishLayout({ children }: { children: React.ReactNode }) {
  const rewritten = (await headers()).get("x-starfish-rewrite");
  if (!rewritten) notFound();

  return children;
}
