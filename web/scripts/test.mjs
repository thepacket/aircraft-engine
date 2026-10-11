// Bundles each test/*.test.ts with esbuild (so the app's extensionless imports
// resolve) and runs them with node's built-in test runner.
import { build } from "esbuild";
import { readdirSync, mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const out = join(root, "node_modules/.cache/tests");
mkdirSync(out, { recursive: true });
const tests = readdirSync(join(root, "test")).filter((f) => f.endsWith(".test.ts"));
await build({ entryPoints: tests.map((f) => join(root, "test", f)), bundle: true, platform: "node", format: "esm", target: "es2022", outdir: out, sourcemap: "inline", logLevel: "error", define: { "import.meta.env.BASE_URL": '"./"' }, external: ["node:*"] });
const r = spawnSync(process.execPath, ["--test", ...tests.map((f) => join(out, f.replace(/\.ts$/, ".js")))], { stdio: "inherit", env: { ...process.env, WEB_ROOT: root } });
process.exit(r.status ?? 1);
