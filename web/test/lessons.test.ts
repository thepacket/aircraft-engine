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
  const text = readFileSync(resolve(root, "public/scenarios", file), "utf8");
  // Two simulators when the script addresses engines (as the IDE with Engines = 2), else one
  const twin = /^\s*(at\s+[0-9.]+|when\s+\S+\s*[<>=!]+\s*\S+)\s+engine\b/im.test(text);
  const sims = Array.from({ length: twin ? 2 : 1 }, () => new Simulator(Simulator.default_spec_json_for(presetKey)));
  const spec = JSON.parse(sims[0].spec_json()) as AnySpec;
  const kind = kindDef(sims[0].kind(), spec);
  setActiveSignals(kind.signals);
  const { events, errors } = parseScenario(text);
  let states = sims.map((s) => JSON.parse(s.state_json()) as AnyState);
  // Commands go to the engine the script addresses; the environment is shared
  let target: number | "all" = "all";
  const each = (f: (s: Simulator) => void): void => { (target === "all" ? sims : [sims[target]].filter(Boolean)).forEach(f); };
  const env = { alt: 0, mach: 0, disa: 0, humidity: 0.4 };
  const setEnv = (a?: number, m?: number, d?: number, hu?: number): void => {
    if (a !== undefined) env.alt = a; if (m !== undefined) env.mach = m; if (d !== undefined) env.disa = d; if (hu !== undefined) env.humidity = hu;
    for (const s of sims) s.set_environment_full(env.alt, env.mach, env.disa, env.humidity);
  };
  const fault = (key: string, value: boolean | number): void => each((s) => s.set_faults_json(JSON.stringify({ [key]: value })));
  const runner = new ScenarioRunner(events, {
    engine: (t) => { target = t; },
    ctl: (name, v) => each((s) => s.set_control(name, v)),
    starter: (on) => each((s) => s.set_control("starter", on ? 1 : 0)),
    fuel: (on) => each((s) => s.set_control(kind.fuelControl.name, on ? kind.fuelControl.on : kind.fuelControl.off)),
    lever: (v) => each((s) => s.set_control(kind.mainLever, v)),
    n1: (p) => { if (kind.kind === "turbofan") each((s) => s.set_control("n1_demand", p ?? -1)); },
    alt: (m) => setEnv(m), mach: (v) => setEnv(undefined, v), isa: (d) => setEnv(undefined, undefined, d), humidity: (x) => setEnv(undefined, undefined, undefined, x),
    running: (lvl) => each((s) => { for (const [name, v] of kind.runningPrep) s.set_control(name, v); if (kind.kind === "turbofan") s.set_control("n1_demand", -1); s.set_running(lvl); }),
    fault: (name, value) => {
      const def = kind.faults.find((f) => f.key === name || f.key === "trigger_" + name || f.key === name + "_qt_per_min" || f.key === name + "_pct");
      if (!def) throw new Error(`unknown fault ${name}`);
      fault(def.key, def.kind === "num" ? parseFloat(value) : parseOnOff(value) === 1);
    },
    bottle: () => fault("fire_bottle", true),
    log: () => {}, speed: () => {}, note: () => {}, screenshot: () => {},
    end: () => {},
  }, (key, t) => signal(key).get((t === "all" ? states[0] : states[t]) ?? states[0]));
  runner.start(states[0].time_s);
  const last = events.reduce((m, e) => Math.max(m, e.t), 0);
  let t = 0;
  while (runner.active && t < last + 120) {
    for (const s of sims) s.step(h);
    t += h;
    states = sims.map((s) => JSON.parse(s.state_json()) as AnyState);
    runner.tick(states[0].time_s);
  }
  for (const s of sims) s.free();
  return { runner, state: states[0], errors, t };
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

test("a watch keeps the engine it was opened on when the script selects another", () => {
  setActiveSignals(kindDef("turbofan").signals);
  const { events, errors } = parseScenario("at 0 engine 1\nat 0 watch egt < 700\nat 1 engine 2\nat 2 check egt < 700\nat 3 end");
  assert.deepEqual(errors, []);
  // engine 1 (index 0) runs hot after t = 1.5; engine 2 (index 1) stays cool
  let t = 0;
  const egt = (i: number): number => (i === 0 && t > 1.5 ? 900 : 500);
  const noop = (): void => {};
  const r = new ScenarioRunner(events, { engine: noop, ctl: noop, starter: noop, fuel: noop, lever: noop, n1: noop, alt: noop, mach: noop, isa: noop, humidity: noop, running: noop, fault: noop, bottle: noop, log: noop, speed: noop, note: noop, screenshot: noop, end: noop },
    (_key, target) => egt(target === "all" ? 0 : target));
  r.start(0);
  for (t = 0; t <= 3.01; t += 0.05) r.tick(t);
  const watch = r.results.find((x) => x.kind === "watch")!, check = r.results.find((x) => x.kind === "check")!;
  assert.equal(watch.pass, false, "the engine-1 watch must see engine 1 run hot: " + watch.detail);
  assert.equal(check.pass, true, "the check after `engine 2` reads engine 2: " + check.detail);
});
