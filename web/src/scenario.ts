// Scenario scripts. Two kinds of lines:
//   at <seconds> <command> [args]                   timed event
//   when <signal> <op> <value> <command> [args]     conditional, armed at the
//        time of the preceding `at` line, fires once when true
// Commands are engine-generic: `ctl <control> <value>` sets any control of the
// active engine kind; shorthands (starter, fuel, tla, ...) map onto it.
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
  end(): void;
}

export const COMMANDS = ["engine", "ctl", "starter", "fuel", "tla", "throttle", "power", "n1", "alt", "mach", "isa", "humidity", "running", "antiice", "pack", "reverser", "fault", "bottle", "log", "speed", "note", "end"];
const OPS = new Set([">", ">=", "<", "<=", "==", "!="]);

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
      lastAt = parseFloat(at[1]);
      events.push({ t: lastAt, cmd, arg: at[3].trim(), line: i + 1 });
    } else if (when) {
      const cmd = when[4].toLowerCase();
      if (!hasSignal(when[1])) { errors.push(`line ${i + 1}: unknown signal "${when[1]}" for this engine`); return; }
      if (!OPS.has(when[2])) { errors.push(`line ${i + 1}: bad operator "${when[2]}"`); return; }
      if (!COMMANDS.includes(cmd)) { errors.push(`line ${i + 1}: unknown command "${when[4]}"`); return; }
      events.push({ t: lastAt, cmd, arg: when[5].trim(), line: i + 1, cond: { signal: when[1], op: when[2], value: parseFloat(when[3]) } });
    } else {
      errors.push(`line ${i + 1}: expected "at <seconds> <command>" or "when <signal> <op> <value> <command>"`);
    }
  });
  events.sort((a, b) => a.t - b.t || a.line - b.line);
  return { events, errors };
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

  constructor(private events: ScenarioEvent[], private h: ScenarioHandlers, private read: (signal: string) => number) {}

  start(simTime: number): void { this.t0 = simTime; this.active = true; this.current = ""; for (const e of this.events) e.fired = false; }
  stop(): void { this.active = false; }
  get elapsed(): number { return this.lastT - this.t0; }
  get pendingConditions(): string[] {
    const rel = this.lastT - this.t0;
    return this.events.filter((e) => e.cond && !e.fired && e.t <= rel).map((e) => `${e.cond!.signal} ${e.cond!.op} ${e.cond!.value} → ${e.cmd} ${e.arg}`);
  }

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
    if (this.events.every((e) => e.fired)) this.active = false;
  }

  private apply(e: ScenarioEvent): void {
    const on = parseOnOff(e.arg) === 1;
    const num = parseFloat(e.arg);
    const n = (d = 0): number => (Number.isFinite(num) ? num : d);
    this.current = `${e.cond ? "when " + e.cond.signal + e.cond.op + e.cond.value : "t+" + e.t + "s"}: ${e.cmd} ${e.arg}`;
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
      case "end": this.h.end(); this.active = false; break;
    }
  }
}
