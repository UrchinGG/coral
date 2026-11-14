import { siteConfig } from "../../shared/astro";

const BACKEND = "http://localhost:8080";
const config = siteConfig("https://admin.urchin.gg", 4323);

export default {
  ...config,
  build: { ...config.build, assets: "assets" },
  vite: {
    build: { assetsInlineLimit: 0 },
    server: {
      proxy: Object.fromEntries(
        ["/api", "/auth", "/login"].map((path) => [path, BACKEND]),
      ),
    },
  },
};
