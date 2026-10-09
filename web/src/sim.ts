// Main-thread client for the simulation worker.
import type { DesignSizing, EngineSpec, EngineState, EngineTarget } from "./types";

type StateListener = (states: EngineState[]) => void;

export class SimClient {
  private worker: Worker;
  private listeners: StateListener[] = [];
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
  private nextId = 1;
  spec!: EngineSpec;
  specJson = "";
  defaultSpecJson = "";
  sizing!: DesignSizing;
  version = "";
  engines: number;
  latest: EngineState[] = [];

  private constructor(engines: number) {
    this.engines = engines;
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  }

  static create(engines: number, spec?: string): Promise<SimClient> {
    return new Promise((resolve, reject) => {
      const c = new SimClient(engines);
      let ready = false;
      c.worker.onmessage = (ev: MessageEvent) => {
        const m = ev.data;
        switch (m.type) {
          case "meta": {
            c.spec = JSON.parse(m.spec);
            c.specJson = m.spec;
            c.sizing = JSON.parse(m.sizing);
            c.version = m.version;
            c.defaultSpecJson = m.defaultSpec;
            if (m.id !== undefined) c.settle(m.id, { spec: c.spec, sizing: c.sizing });
            if (!ready) { ready = true; resolve(c); }
            break;
          }
          case "state": {
            c.latest = (m.states as string[]).map((s) => JSON.parse(s) as EngineState);
            for (const l of c.listeners) l(c.latest);
            break;
          }
          case "csv": c.settle(m.id, m.csv); break;
          case "error": {
            if (m.id !== undefined) c.fail(m.id, new Error(m.message));
            else { console.error("worker:", m.message); if (!ready) reject(new Error(m.message)); }
            break;
          }
        }
      };
      c.worker.onerror = (e) => { console.error(e); if (!ready) reject(e); };
      c.worker.postMessage({ type: "init", engines, spec });
    });
  }

  private settle(id: number, v: unknown): void { const p = this.pending.get(id); if (p) { this.pending.delete(id); p.resolve(v); } }
  private fail(id: number, e: unknown): void { const p = this.pending.get(id); if (p) { this.pending.delete(id); p.reject(e); } }
  private request<T>(msg: Record<string, unknown>): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.worker.postMessage({ ...msg, id });
    });
  }

  onState(l: StateListener): void { this.listeners.push(l); }

  cmd(name: string, args: unknown[] = [], target: EngineTarget = "all"): void {
    this.worker.postMessage({ type: "cmd", target, name, args });
  }
  setSpeed(v: number): void { this.worker.postMessage({ type: "speed", value: v }); }
  setPaused(v: boolean): void { this.worker.postMessage({ type: "pause", value: v }); }
  reset(): void { this.worker.postMessage({ type: "reset" }); }
  setEngines(n: number): void { this.engines = n; this.worker.postMessage({ type: "engines", count: n }); }
  async setSpec(json: string): Promise<{ spec: EngineSpec; sizing: DesignSizing }> {
    const r = await this.request<{ spec: EngineSpec; sizing: DesignSizing }>({ type: "spec", json });
    this.specJson = JSON.stringify(r.spec, null, 2);
    return r;
  }
  csv(engine: number): Promise<string> { return this.request<string>({ type: "csv", engine }); }
  setFaults(target: EngineTarget, patch: Record<string, unknown>): void { this.worker.postMessage({ type: "faults", target, patch }); }
  clearFaults(target: EngineTarget): void { this.worker.postMessage({ type: "faults", target, patch: null }); }
  // Convenience wrappers
  setThrottle(v: number, t: EngineTarget = "all"): void { this.cmd("set_throttle", [v], t); }
  setFuelLever(on: boolean, t: EngineTarget = "all"): void { this.cmd("set_fuel_lever", [on], t); }
  setStarter(on: boolean, t: EngineTarget = "all"): void { this.cmd("set_starter", [on], t); }
  setAntiIce(on: boolean, t: EngineTarget = "all"): void { this.cmd("set_anti_ice", [on], t); }
  setPackBleed(on: boolean, t: EngineTarget = "all"): void { this.cmd("set_pack_bleed", [on], t); }
  setReverser(on: boolean, t: EngineTarget = "all"): void { this.cmd("set_reverser", [on], t); }
  setN1Demand(pct: number, t: EngineTarget = "all"): void { this.cmd("set_n1_demand", [pct], t); }
  setEnvironment(alt: number, mach: number, disa: number): void { this.cmd("set_environment", [alt, mach, disa]); }
  setRunning(tla: number, t: EngineTarget = "all"): void { this.cmd("set_running", [tla], t); }
  loggerStart(): void { this.cmd("logger_start"); }
  loggerStop(): void { this.cmd("logger_stop"); }
  loggerClear(): void { this.cmd("logger_clear"); }
  loggerRate(hz: number): void { this.cmd("logger_set_rate", [hz]); }
}
