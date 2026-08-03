import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Server proxies /api during local dev (`npm run dev`); in the built
// container the Rust server serves both the static bundle and /api itself,
// so no proxy is needed there.
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": "http://localhost:8080",
    },
  },
});
