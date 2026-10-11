// Flight-log import round trips: every engine family's own logger CSV must
// import with the right columns and units, quoted CSVs must parse, and the
// fuel control / lever columns must map per family.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { initSync, Simulator } from "../src/wasm/engine_wasm.js";
import { parseCsv, autoMap, suffixUnit, measuredSignals, Replay } from "../src/replay";
import { setActiveSignals, signal } from "../src/signals";
import { kindDef } from "../src/kinds";
import { speedOfSound } from "../src/units";
import type { AnyState } from "../src/types";

// WEB_ROOT is set by scripts/test.mjs (the bundle runs from a cache directory)
initSync({ module: readFileSync(resolve(process.env.WEB_ROOT ?? ".", "src/wasm/engine_wasm_bg.wasm")) });

function nativeLog(kind: string, seconds = 6): { csv: string; state: AnyState } {
  const sim = new Simulator(Simulator.default_spec_json_for(kind));
  sim.set_environment_full(1500, 0.2, 5, 0.4);
  sim.set_running(0.8);
  sim.logger_set_rate(5);
  sim.logger_start();
  for (let i = 0; i < seconds / 0.02; i++) sim.step(0.02);
  const csv = sim.logger_csv();
  const state = JSON.parse(sim.state_json()) as AnyState;
  sim.free();
  return { csv, state };
}

for (const kind of ["turbofan", "piston", "turboprop", "turbojet", "turboshaft"]) {
  test(`${kind}: the simulator's own CSV imports with the right units`, () => {
    const { csv, state } = nativeLog(kind);
    setActiveSignals(kindDef(kind).signals);
    const table = parseCsv(csv);
    const map = autoMap(table.columns);
    const replay = new Replay(table, map);
    const s = replay.sample(replay.duration);
    let checked = 0;
    for (const [key, measured] of Object.entries(s.measured)) {
      const actual = signal(key).get(state);
      if (!Number.isFinite(actual)) continue;
      // The last logged row is at most one logger period (0.2 s) before the final state
      const tol = 0.05 * Math.abs(actual) + (Math.abs(actual) < 1 ? 0.05 : 0.5);
      assert.ok(Math.abs(measured - actual) <= tol, `${kind} ${key} from column "${map.columns[key]}" (unit ${map.units[key]}): imported ${measured}, state ${actual}`);
      checked++;
    }
    assert.ok(checked >= 5, `${kind}: only ${checked} signals round-tripped`);
    // Every importable signal whose native column is in the log must be mapped
    for (const sg of measuredSignals()) {
      const native = table.columns.find((c) => sg.patterns![0].test(c));
      if (native) assert.equal(map.columns[sg.key], native, `${kind}: native column ${native} for ${sg.key} not mapped (${map.notes[sg.key] ?? "no note"})`);
    }
    assert.ok(s.measured.thrust === undefined || Math.abs(s.measured.thrust) < 200, `${kind}: thrust ${s.measured.thrust} kN looks like newtons`);
  });
}

test("turboshaft lever maps to the collective and the twist grip is replayed as logged", () => {
  const { csv } = nativeLog("turboshaft");
  setActiveSignals(kindDef("turboshaft").signals);
  const table = parseCsv(csv);
  const map = autoMap(table.columns);
  assert.equal(map.columns.lever, "collective");
  assert.equal(map.columns.fuel_lever, "condition_lever");
  const s = new Replay(table, map).sample(1);
  assert.equal(s.fuel_value, 2, "twist grip FULL (2) must not collapse to a boolean");
  assert.ok(s.lever !== undefined && s.lever > 0.5, `collective ${s.lever}`);
});

test("piston mixture is replayed as a value, not a boolean", () => {
  const { csv } = nativeLog("piston");
  setActiveSignals(kindDef("piston").signals);
  const table = parseCsv(csv);
  const map = autoMap(table.columns);
  assert.equal(map.columns.fuel_lever, "mixture");
  const s = new Replay(table, map).sample(1);
  assert.ok(s.fuel_value !== undefined && s.fuel_value > 0.5 && s.fuel_value <= 1, `mixture ${s.fuel_value}`);
});

test("column suffixes name their units", () => {
  assert.equal(suffixUnit("thrust_n"), "N");
  assert.equal(suffixUnit("thrust_kn"), "kN");
  assert.equal(suffixUnit("torque_ftlb"), "ft·lb");
  assert.equal(suffixUnit("torque_pct"), "%");
  assert.equal(suffixUnit("egt_c"), "°C");
  assert.equal(suffixUnit("fuel_flow_pph"), "lb/h");
  assert.equal(suffixUnit("n1"), undefined);
});

test("a percent-torque engine picks torque_pct over torque_ftlb", () => {
  setActiveSignals(kindDef("turboshaft").signals);
  const map = autoMap(["time_s", "torque_ftlb", "torque_pct", "ng_pct"]);
  assert.equal(map.columns.torque, "torque_pct");
  setActiveSignals(kindDef("turboprop").signals);
  const map2 = autoMap(["time_s", "torque_ftlb", "torque_pct", "ng_pct"]);
  assert.equal(map2.columns.torque, "torque_ftlb");
  assert.equal(map2.units.torque, 0);
});

test("a column in a unit the engine cannot take is left unmapped with a note", () => {
  setActiveSignals(kindDef("turboshaft").signals);
  const map = autoMap(["time_s", "torque_ftlb", "ng_pct"]);
  assert.equal(map.columns.torque, undefined);
  assert.match(map.notes.torque, /torque_ftlb is in ft·lb/);
});

test("quoted CSV values parse and bad time columns are rejected", () => {
  const t = parseCsv('"time_s","n1_pct","egt_c"\n"0","20","400"\n"1","21","410"\n"2","22","420"\n');
  assert.deepEqual([...t.rows[1]], [20, 21, 22]);
  setActiveSignals(kindDef("turbofan").signals);
  const map = autoMap(t.columns);
  const r = new Replay(t, map);
  assert.equal(r.duration, 2);
  assert.equal(r.sample(1).measured.n1, 21);
  const bad = parseCsv("time_s,n1_pct\n0,20\n2,21\n1,22\n");
  assert.throws(() => new Replay(bad, autoMap(bad.columns)), /not increasing/);
  const nan = parseCsv("time_s,n1_pct\n0,20\nx,21\n");
  assert.throws(() => new Replay(nan, autoMap(nan.columns)), /non-numeric/);
});

test("speed of sound matches the core's ISA", () => {
  assert.ok(Math.abs(speedOfSound(0) - 340.29) < 0.05);
  assert.ok(Math.abs(speedOfSound(5000) - 320.53) < 0.1);
  assert.ok(Math.abs(speedOfSound(11000) - 295.07) < 0.1);
  assert.ok(Math.abs(speedOfSound(15000) - 295.07) < 0.1);
});
