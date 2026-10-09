// Dedicated worker: owns the WebAssembly simulators and runs the clock.
// Workers are not throttled when the tab is in the background, so a lesson
// keeps running while the student reads something else.
import init, { Simulator } from "./wasm/engine_wasm.js";

type Msg =
  | { type: "init"; engines: number; spec?: string }
  | { type: "cmd"; target: number | "all"; name: string; args: unknown[] }
  | { type: "speed"; value: number }
  | { type: "pause"; value: boolean }
  | { type: "reset" }
  | { type: "engines"; count: number }
  | { type: "spec"; json: string; id: number }
  | { type: "kind"; kind: string; id: number }
  | { type: "csv"; engine: number; id: number }
  | { type: "faults"; target: number | "all"; patch: Record<string, unknown> | null };

let sims: Simulator[] = [];
let speed = 1;
let paused = false;
let lastReal = 0;
let lastPost = 0;
let specJson = "";

function makeSims(n: number): void {
  sims = [];
  for (let i = 0; i < n; i++) sims.push(new Simulator(specJson));
}

function postMeta(id?: number): void {
  const s = sims[0];
  (self as unknown as Worker).postMessage({ type: "meta", id, kind: s.kind(), spec: s.spec_json(), sizing: s.sizing_json(), version: Simulator.version(), defaultSpec: Simulator.default_spec_json_for(s.kind()) });
}

function tick(): void {
  const now = performance.now();
  const realDt = Math.min((now - lastReal) / 1000, 0.5);
  lastReal = now;
  if (!paused && realDt > 0) {
    const dt = realDt * speed;
    for (const s of sims) s.step(dt);
  }
  if (now - lastPost >= 33) {
    lastPost = now;
    const states = sims.map((s) => s.state_json());
    (self as unknown as Worker).postMessage({ type: "state", states });
  }
}

self.onmessage = async (ev: MessageEvent<Msg>) => {
  const m = ev.data;
  try {
    switch (m.type) {
      case "init": {
        await init();
        specJson = m.spec ?? "";
        makeSims(m.engines);
        postMeta();
        lastReal = performance.now();
        setInterval(tick, 16);
        break;
      }
      case "cmd": {
        const targets = m.target === "all" ? sims : [sims[m.target]].filter(Boolean);
        for (const s of targets) {
          const fn = (s as unknown as Record<string, (...a: unknown[]) => unknown>)[m.name];
          if (typeof fn !== "function") throw new Error(`unknown command ${m.name}`);
          fn.apply(s, m.args);
        }
        break;
      }
      case "speed": speed = m.value; break;
      case "pause": paused = m.value; break;
      case "reset": for (const s of sims) s.reset(); break;
      case "engines": {
        const keep = sims.length;
        if (m.count !== keep) makeSims(m.count);
        break;
      }
      case "spec": {
        // Validate on a scratch instance first so a bad spec cannot kill the session
        const probe = new Simulator(m.json);
        probe.free();
        specJson = m.json;
        for (const s of sims) s.set_spec_json(m.json);
        postMeta(m.id);
        break;
      }
      case "kind": {
        specJson = Simulator.default_spec_json_for(m.kind);
        for (const s of sims) s.set_spec_json(specJson);
        postMeta(m.id);
        break;
      }
      case "csv": {
        const s = sims[m.engine];
        (self as unknown as Worker).postMessage({ type: "csv", id: m.id, csv: s ? s.logger_csv() : "" });
        break;
      }
      case "faults": {
        const targets = m.target === "all" ? sims : [sims[m.target]].filter(Boolean);
        for (const s of targets) {
          if (m.patch === null) s.clear_faults(); else s.set_faults_json(JSON.stringify(m.patch));
        }
        break;
      }
    }
  } catch (e) {
    (self as unknown as Worker).postMessage({ type: "error", id: (m as { id?: number }).id, message: String(e) });
  }
};
