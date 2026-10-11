// Every lesson offered for every preset is run against the real wasm with the
// scenario runner, and must complete with every check and watch passed. This
// is the proof that the lessons demonstrate what they claim; a lesson whose
// event never happens fails here as "not reached".
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync, Simulator } from "../src/wasm/engine_wasm.js";
import { parseScenario, parseOnOff, ScenarioRunner } from "../src/scenario";
import { setActiveSignals, signal } from "../src/signals";
import { kindDef, lessonsFor } from "../src/kinds";
import type { AnyState, AnySpec } from "../src/types";

const root = process.env.WEB_ROOT ?? ".";
initSync({ module: readFileSync(resolve(root, "src/wasm/engine_wasm_bg.wasm")) });

interface Preset { key: string; kind: string; label: string }
const presets = JSON.parse(Simulator.presets_json()) as Preset[];

/** Run one lesson on one preset; returns the runner and the final state. */
export function runLesson(presetKey: string, file: string, h = 0.05): { runner: ScenarioRunner; state: AnyState; errors: string[]; t: number } {
  const sim = new Simulator(Simulator.default_spec_json_for(presetKey));
  const spec = JSON.parse(sim.spec_json()) as AnySpec;
  const kind = kindDef(sim.kind(), spec);
  setActiveSignals(kind.signals);
  const text = readFileSync(resolve(root, "public/scenarios", file), "utf8");
  const { events, errors } = parseScenario(text);
  let state = JSON.parse(sim.state_json()) as AnyState;
  const env = { alt: 0, mach: 0, disa: 0, humidity: 0.4 };
  const setEnv = (a?: number, m?: number, d?: number, hu?: number): void => {
    if (a !== undefined) env.alt = a; if (m !== undefined) env.mach = m; if (d !== undefined) env.disa = d; if (hu !== undefined) env.humidity = hu;
    sim.set_environment_full(env.alt, env.mach, env.disa, env.humidity);
  };
  const fault = (key: string, value: boolean | number): void => sim.set_faults_json(JSON.stringify({ [key]: value }));
  const runner = new ScenarioRunner(events, {
    engine: () => {},
    ctl: (name, v) => sim.set_control(name, v),
    starter: (on) => sim.set_control("starter", on ? 1 : 0),
    fuel: (on) => sim.set_control(kind.fuelControl.name, on ? kind.fuelControl.on : kind.fuelControl.off),
    lever: (v) => sim.set_control(kind.mainLever, v),
    n1: (p) => { if (kind.kind === "turbofan") sim.set_control("n1_demand", p ?? -1); },
    alt: (m) => setEnv(m), mach: (v) => setEnv(undefined, v), isa: (d) => setEnv(undefined, undefined, d), humidity: (x) => setEnv(undefined, undefined, undefined, x),
    running: (lvl) => { for (const [name, v] of kind.runningPrep) sim.set_control(name, v); if (kind.kind === "turbofan") sim.set_control("n1_demand", -1); sim.set_running(lvl); },
    fault: (name, value) => {
      const def = kind.faults.find((f) => f.key === name || f.key === "trigger_" + name || f.key === name + "_qt_per_min" || f.key === name + "_pct");
      if (!def) throw new Error(`unknown fault ${name}`);
      fault(def.key, def.kind === "num" ? parseFloat(value) : parseOnOff(value) === 1);
    },
    bottle: () => fault("fire_bottle", true),
    log: () => {}, speed: () => {}, note: () => {}, screenshot: () => {},
    end: () => {},
  }, (key) => signal(key).get(state));
  runner.start(state.time_s);
  const last = events.reduce((m, e) => Math.max(m, e.t), 0);
  let t = 0;
  while (runner.active && t < last + 120) {
    sim.step(h);
    t += h;
    state = JSON.parse(sim.state_json()) as AnyState;
    runner.tick(state.time_s);
  }
  sim.free();
  return { runner, state, errors, t };
}

for (const p of presets) {
  const spec = JSON.parse(Simulator.default_spec_json_for(p.key)) as AnySpec;
  for (const [file, label] of lessonsFor(kindDef(p.kind, spec), p.key)) {
    test(`${p.key}: ${label} (${file})`, () => {
      const { runner, errors, t } = runLesson(p.key, file);
      assert.deepEqual(errors, [], `script errors: ${errors.join("; ")}`);
      assert.ok(!runner.active, `lesson did not finish within ${t.toFixed(0)} s; waiting on: ${runner.pendingConditions.join(", ")}`);
      const bad = runner.results.filter((r) => r.pass !== true).map((r) => `line ${r.line}: ${r.kind} ${r.text} (${r.detail})`);
      assert.deepEqual(bad, [], `failed checks:\n  ${bad.join("\n  ")}`);
    });
  }
}
