import { defineConfig, type Plugin } from "vite";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/**
 * Emits `sw.js` with the list of built files to precache (every JS, CSS, wasm
 * and worker chunk plus the static files in public/) and a cache name that
 * changes with the build, so offline use does not depend on which files
 * happened to be fetched before the service worker took control.
 */
function serviceWorker(): Plugin {
  const publicFiles = (dir: string, prefix = ""): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? publicFiles(join(dir, e.name), `${prefix}${e.name}/`) : [`${prefix}${e.name}`]));
  return {
    name: "service-worker-precache",
    apply: "build",
    generateBundle(_opts, bundle) {
      const built = Object.keys(bundle).filter((f) => /\.(js|css|wasm)$/.test(f) && f !== "sw.js");
      const statics = publicFiles(join(__dirname, "public")).filter((f) => f !== "sw.js");
      const files = ["./", ...built, ...statics].map((f) => (f.startsWith("./") ? f : `./${f}`));
      const build = createHash("sha1").update(files.join("\n")).digest("hex").slice(0, 12);
      const src = readFileSync(join(__dirname, "src/sw.js"), "utf8");
      this.emitFile({ type: "asset", fileName: "sw.js", source: `self.__PRECACHE__ = ${JSON.stringify(files)};\nself.__BUILD__ = ${JSON.stringify(build)};\n${src}` });
    },
  };
}

// Relative base so the built `dist/` can be served from any path by any
// static file server (nginx, GitHub Pages, python -m http.server, ...).
export default defineConfig({
  base: "./",
  plugins: [serviceWorker()],
  build: {
    target: "es2022",
    outDir: "dist",
    assetsInlineLimit: 0,
  },
  server: {
    port: 5173,
    fs: { allow: [".."] },
    headers: {
      // Not strictly required (no SharedArrayBuffer), but harmless and lets
      // the page opt into cross-origin isolation later if a worker needs it.
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
