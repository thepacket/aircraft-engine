// Scenario scripts. Two kinds of lines:
//   at <seconds> <command> [args]                   timed event
//   when <signal> <op> <value> <command> [args]     conditional, armed at the
//        time of the preceding `at` line, fires once when true
// Commands are engine-generic: `ctl <control> <value>` sets any control of the
// active engine kind; shorthands (starter, fuel, tla, ...) map onto it.
// Checkpoints turn a lesson into an assessment:
//   check <signal> <op> <value>     evaluated once, when the line fires
//   watch <signal> <op> <value>     must hold from when the line fires to the end
import { hasSignal } from "./signals";

export interface ScenarioEvent {
  t: number; cmd: string; arg: string; line: number;
  cond?: { signal: string; op: string; value: number };
  fired?: boolean;
}

export interface ScenarioHandlers {
  engine(target: number | "all"): void;
  ctl(name: string, value: number): void;
  starter(on: boolean): void;
  fuel(on: boolean): void;
  lever(v: number): void;
  n1(pct: number | null): void;
  alt(m: number): void;
  mach(v: number): void;
  isa(dK: number): void;
  humidity(x: number): void;
  running(level: number): void;
  fault(name: string, value: string): void;
  bottle(): void;
  log(on: boolean): void;
  speed(x: number): void;
  note(text: string): void;
  /** Save a PNG of the flight deck; the optional label goes into the file name */
  screenshot(label: string): void;
  end(): void;
}

export const COMMANDS = ["engine", "ctl", "starter", "fuel", "tla", "throttle", "power", "n1", "alt", "mach", "isa", "humidity", "running", "antiice", "pack", "reverser", "fault", "bottle", "log", "speed", "note", "screenshot", "check", "watch", "end"];
const OPS = new Set([">", ">=", "<", "<=", "==", "!="]);

export interface Condition { signal: string; op: string; value: number }
export interface CheckResult {
  line: number;
  kind: "check" | "watch";
  text: string;
  /** null while a watch is still open */
  pass: boolean | null;
  detail: string;
}

function parseCondition(arg: string): Condition | null {
  const m = arg.match(/^(\w+)\s*(>=|<=|==|!=|>|<)\s*(-?[0-9.]+)\s*$/);
  return m && OPS.has(m[2]) ? { signal: m[1], op: m[2], value: parseFloat(m[3]) } : null;
}

function validateCheck(cmd: string, arg: string, i: number, errors: string[]): boolean {
  if (cmd !== "check" && cmd !== "watch") return true;
  const c = parseCondition(arg);
  if (!c) { errors.push(`line ${i + 1}: expected "${cmd} <signal> <op> <value>"`); return false; }
  if (!hasSignal(c.signal)) { errors.push(`line ${i + 1}: unknown signal "${c.signal}" for this engine`); return false; }
  return true;
}

export function parseScenario(text: string): { events: ScenarioEvent[]; errors: string[] } {
  const events: ScenarioEvent[] = [];
  const errors: string[] = [];
  let lastAt = 0;
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.split("#")[0].trim();
    if (!line) return;
    const at = line.match(/^at\s+([0-9.]+)\s+(\S+)\s*(.*)$/i);
    const when = line.match(/^when\s+(\w+)\s*(>=|<=|==|!=|>|<)\s*(-?[0-9.]+)\s+(\S+)\s*(.*)$/i);
    if (at) {
      const cmd = at[2].toLowerCase();
      if (!COMMANDS.includes(cmd)) { errors.push(`line ${i + 1}: unknown command "${at[2]}"`); return; }
      if (!validateCheck(cmd, at[3].trim(), i, errors)) return;
      lastAt = parseFloat(at[1]);
      events.push({ t: lastAt, cmd, arg: at[3].trim(), line: i + 1 });
    } else if (when) {
      const cmd = when[4].toLowerCase();
      if (!hasSignal(when[1])) { errors.push(`line ${i + 1}: unknown signal "${when[1]}" for this engine`); return; }
      if (!OPS.has(when[2])) { errors.push(`line ${i + 1}: bad operator "${when[2]}"`); return; }
      if (!COMMANDS.includes(cmd)) { errors.push(`line ${i + 1}: unknown command "${when[4]}"`); return; }
      if (!validateCheck(cmd, when[5].trim(), i, errors)) return;
      events.push({ t: lastAt, cmd, arg: when[5].trim(), line: i + 1, cond: { signal: when[1], op: when[2], value: parseFloat(when[3]) } });
    } else {
      errors.push(`line ${i + 1}: expected "at <seconds> <command>" or "when <signal> <op> <value> <command>"`);
    }
  });
  events.sort((a, b) => a.t - b.t || a.line - b.line);
  return { events, errors };
}

function fmtNum(v: number): string {
  if (!Number.isFinite(v)) return "n/a";
  const a = Math.abs(v);
  return a >= 100 ? v.toFixed(0) : a >= 10 ? v.toFixed(1) : v.toFixed(2);
}

function test(op: string, a: number, b: number): boolean {
  switch (op) {
    case ">": return a > b;
    case ">=": return a >= b;
    case "<": return a < b;
    case "<=": return a <= b;
    case "==": return Math.abs(a - b) < 1e-9;
    case "!=": return Math.abs(a - b) >= 1e-9;
  }
  return false;
}

export function parseOnOff(arg: string): number | null {
  if (/^(on|1|true|run|deploy|hot|both)$/i.test(arg)) return 1;
  if (/^(off|0|false|cutoff|cold|stow)$/i.test(arg)) return 0;
  const v = parseFloat(arg);
  return Number.isFinite(v) ? v : null;
}

export class ScenarioRunner {
  private t0 = 0;
  private lastT = 0;
  active = false;
  current = "";
  /** Script line (1-based) of the last event applied, 0 before the first */
  currentLine = 0;
  /** Checkpoint results in the order they were opened */
  results: CheckResult[] = [];
  private watches: { result: CheckResult; cond: Condition; worst: number; worstT: number }[] = [];

  constructor(private events: ScenarioEvent[], private h: ScenarioHandlers, private read: (signal: string) => number) {}

  /** True when the run was stopped before every event fired */
  interrupted = false;
  start(simTime: number): void { this.t0 = simTime; this.lastT = simTime; this.active = true; this.interrupted = false; this.current = ""; this.currentLine = 0; this.results = []; this.watches = []; for (const e of this.events) e.fired = false; }
  /**
   * Stop early. Open watches stay open (they were not held to the end) and the
   * run is marked interrupted, so it cannot be graded as a complete run.
   */
  stop(): void {
    if (!this.active) return;
    this.interrupted = !this.events.every((e) => e.fired);
    for (const w of this.watches) if (w.result.pass === null) w.result.detail = `stopped at t+${this.elapsed.toFixed(0)} s`;
    this.watches = [];
    this.active = false;
  }
  /** Checks and watches the script still had ahead of it when it was stopped */
  get skipped(): number { return this.events.filter((e) => !e.fired && (e.cmd === "check" || e.cmd === "watch")).length; }
  /** Close the open watches (they pass if never violated) and stop. */
  private finish(): void {
    for (const w of this.watches) if (w.result.pass === null) { w.result.pass = true; w.result.detail = `held to t+${this.elapsed.toFixed(0)} s`; }
    this.watches = [];
    this.active = false;
  }
  get summary(): { passed: number; failed: number; pending: number } {
    let passed = 0, failed = 0, pending = 0;
    for (const r of this.results) { if (r.pass === null) pending++; else if (r.pass) passed++; else failed++; }
    return { passed, failed, pending };
  }
  get elapsed(): number { return this.lastT - this.t0; }
  private get armed(): ScenarioEvent[] { const rel = this.lastT - this.t0; return this.events.filter((e) => e.cond && !e.fired && e.t <= rel); }
  get pendingConditions(): string[] { return this.armed.map((e) => `${e.cond!.signal} ${e.cond!.op} ${e.cond!.value} → ${e.cmd} ${e.arg}`); }
  /** Script lines of the conditional events waiting to fire */
  get waitingLines(): number[] { return this.armed.map((e) => e.line); }

  tick(simTime: number): void {
    if (!this.active) return;
    this.lastT = simTime;
    const rel = simTime - this.t0;
    for (const e of this.events) {
      if (e.fired || e.t > rel + 1e-9) continue;
      if (e.cond && !test(e.cond.op, this.read(e.cond.signal), e.cond.value)) continue;
      e.fired = true;
      this.apply(e);
      if (!this.active) return;
    }
    this.evalWatches(rel);
    if (this.events.every((e) => e.fired)) this.finish();
  }

  /** Watches: record the first violation and the worst value seen */
  private evalWatches(rel: number): void {
    for (const w of this.watches) {
      const v = this.read(w.cond.signal);
      if (!Number.isFinite(v)) continue;
      const bad = !test(w.cond.op, v, w.cond.value);
      const worse = w.worst === undefined || Number.isNaN(w.worst) || (/[<]/.test(w.cond.op) ? v > w.worst : /[>]/.test(w.cond.op) ? v < w.worst : Math.abs(v - w.cond.value) > Math.abs(w.worst - w.cond.value));
      if (bad && (w.result.pass !== false || worse)) { w.worst = v; w.worstT = rel; w.result.pass = false; w.result.detail = `${w.cond.signal} = ${fmtNum(v)} at t+${rel.toFixed(0)} s`; }
    }
  }

  private apply(e: ScenarioEvent): void {
    const on = parseOnOff(e.arg) === 1;
    const num = parseFloat(e.arg);
    const n = (d = 0): number => (Number.isFinite(num) ? num : d);
    this.current = `${e.cond ? "when " + e.cond.signal + e.cond.op + e.cond.value : "t+" + e.t + "s"}: ${e.cmd} ${e.arg}`;
    this.currentLine = e.line;
    switch (e.cmd) {
      case "engine": this.h.engine(/^(all|both)$/i.test(e.arg) ? "all" : Math.max(0, n(1) - 1)); break;
      case "ctl": { const [name, ...rest] = e.arg.split(/\s+/); const v = parseOnOff(rest.join(" ") || "1"); if (v !== null) this.h.ctl(name, v); break; }
      case "starter": this.h.starter(on); break;
      case "fuel": this.h.fuel(on); break;
      case "tla": case "throttle": case "power": this.h.lever(n()); break;
      case "n1": this.h.n1(/^off$/i.test(e.arg) ? null : n()); break;
      case "alt": this.h.alt(n()); break;
      case "mach": this.h.mach(n()); break;
      case "isa": this.h.isa(n()); break;
      case "humidity": this.h.humidity(n()); break;
      case "running": this.h.running(n()); break;
      case "antiice": this.h.ctl("anti_ice", on ? 1 : 0); break;
      case "pack": this.h.ctl("pack_bleed", on ? 1 : 0); break;
      case "reverser": this.h.ctl("reverser", on ? 1 : 0); break;
      case "fault": { const [name, ...rest] = e.arg.split(/\s+/); this.h.fault(name, rest.join(" ") || "on"); break; }
      case "bottle": this.h.bottle(); break;
      case "log": this.h.log(on); break;
      case "speed": this.h.speed(n(1)); break;
      case "note": this.h.note(e.arg); break;
      case "screenshot": this.h.screenshot(e.arg); break;
      case "check": {
        const c = parseCondition(e.arg)!;
        const v = this.read(c.signal);
        const pass = Number.isFinite(v) && test(c.op, v, c.value);
        this.results.push({ line: e.line, kind: "check", text: e.arg, pass, detail: `${c.signal} = ${fmtNum(v)} at t+${(this.lastT - this.t0).toFixed(0)} s` });
        break;
      }
      case "watch": {
        const c = parseCondition(e.arg)!;
        const result: CheckResult = { line: e.line, kind: "watch", text: e.arg, pass: null, detail: "" };
        this.results.push(result);
        this.watches.push({ result, cond: c, worst: NaN, worstT: 0 });
        break;
      }
      // The ending tick still counts for the watches, so a limit busted on the
      // same tick as `end` is not missed.
      case "end": this.evalWatches(this.lastT - this.t0); this.h.end(); this.finish(); break;
    }
  }
}
