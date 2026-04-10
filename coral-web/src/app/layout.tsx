import type { Metadata } from "next";
import { Inter } from "next/font/google";
import { OceanBackground } from "@/components/OceanBackground";
import "./globals.css";

const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  display: "swap",
});

const siteUrl = process.env.SITE_URL || "https://coral.urchin.gg";

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: { default: "Coral", template: "%s — Coral" },
  description: "Hypixel stats, player blacklist, and tools — by Coral.",
  keywords: ["Hypixel", "Minecraft", "stats", "Bedwars", "SkyWars", "Duels", "blacklist", "Coral"],
  openGraph: {
    type: "website",
    url: siteUrl,
    siteName: "Coral",
    title: "Coral — Hypixel Stats & Tools",
    description: "Player stats, community blacklist, and tools for Hypixel.",
    images: [{ url: "/api/og/site", width: 1200, height: 630, alt: "Coral" }],
  },
  twitter: {
    card: "summary_large_image",
    title: "Coral — Hypixel Stats & Tools",
    description: "Player stats, community blacklist, and tools for Hypixel.",
    images: ["/api/og/site"],
  },
  alternates: { canonical: siteUrl },
  icons: { icon: "/favicon.ico" },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <link rel="preload" as="font" type="font/woff2" href="/fonts/minecraft.woff2" crossOrigin="anonymous" />
        <link rel="preload" as="font" type="font/woff2" href="/fonts/minecraft-bold.woff2" crossOrigin="anonymous" />
      </head>
      <body className={`${inter.variable} antialiased`}>
        <OceanBackground />
        {children}
      </body>
    </html>
  );
}
