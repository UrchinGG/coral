import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

import { precompressWithBudgets } from "./build/precompress.ts";

export default defineConfig({
  plugins: [react(), tailwindcss(), precompressWithBudgets()],
  server: {
    proxy: {
      "/api": "http://localhost:8080",
      "/auth": "http://localhost:8080",
      "/login": "http://localhost:8080",
    },
  },
  build: {
    outDir: "dist",
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: "vendor", test: /[\\/]node_modules[\\/]/, tags: ["$initial"] }],
        },
      },
    },
  },
});
