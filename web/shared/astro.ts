import { defineConfig, envField, fontProviders } from "astro/config";

const url = (fallback: string) =>
  envField.string({ context: "server", access: "public", default: fallback });

export function siteConfig(site: string, port: number) {
  return defineConfig({
    site,
    trailingSlash: "never",
    build: { format: "file" },
    server: { port },
    env: {
      schema: {
        CORAL_API_URL: url("https://api.urchin.gg"),
        URCHIN_URL: url("https://urchin.gg"),
        STARFISH_URL: url("https://starfish.urchin.gg"),
      },
    },
    fonts: [
      {
        provider: fontProviders.fontsource(),
        name: "Inter",
        cssVariable: "--font-inter",
        weights: ["400 700"],
        styles: ["normal"],
        subsets: ["latin"],
      },
      {
        provider: fontProviders.fontsource(),
        name: "JetBrains Mono",
        cssVariable: "--font-jetbrains-mono",
        weights: ["400 500"],
        styles: ["normal"],
        subsets: ["latin"],
      },
    ],
  });
}
