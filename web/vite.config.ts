import { defineConfig } from "vite";

// Relative base so the built `dist/` can be served from any path by any
// static file server (nginx, GitHub Pages, python -m http.server, ...).
export default defineConfig({
  base: "./",
  build: {
    target: "es2022",
    outDir: "dist",
    assetsInlineLimit: 0,
  },
  server: {
    port: 5173,
    headers: {
      // Not strictly required (no SharedArrayBuffer), but harmless and lets
      // the page opt into cross-origin isolation later if a worker needs it.
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
