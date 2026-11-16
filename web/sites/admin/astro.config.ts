import { siteConfig } from "../../shared/astro";

const BACKEND = "http://localhost:8080";
const config = siteConfig("https://urchin.gg", 4323);

export default {
  ...config,
  base: "/dashboard",
  build: { ...config.build, assets: "assets" },
  vite: {
    build: { assetsInlineLimit: 0 },
    server: {
      proxy: Object.fromEntries(
        ["/dashboard/api", "/auth", "/me"].map((path) => [path, BACKEND]),
      ),
    },
  },
};
