// Scenario scripts. Two kinds of lines:
//   at <seconds> <command> [args]           timed event
//   when <signal> <op> <value> <command> [args]   conditional event, armed at
//        the time of the preceding `at` line, fires once when true
// Same `at` syntax as the native engine-cli.
import { hasSignal } from "./signals";

export interface ScenarioEvent {
  t: number;
  cmd: string;
  arg: string;
  line: number;
  cond?: { signal: string; op: string; value: number };
  fired?: boolean;
}

export interface ScenarioHandlers {
  engine(target: number | "all"): void;
  starter(on: boolean): void;
  fuel(on: boolean): void;
  tla(v: number): void;
  n1(pct: number | null): void;
  alt(m: number): void;
  mach(v: number): void;
  isa(dK: number): void;
  running(tla: number): void;
  antiice(on: boolean): void;
  pack(on: boolean): void;
  reverser(on: boolean): void;
  fault(name: string, value: string): void;
  bottle(): void;
  log(on: boolean): void;
  speed(x: number): void;
  note(text: string): void;
  end(): void;
}

export const COMMANDS = ["engine", "starter", "fuel", "tla", "n1", "alt", "mach", "isa", "running", "antiice", "pack", "reverser", "fault", "bottle", "log", "speed", "note", "end"];
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
      if (!hasSignal(when[1])) { errors.push(`line ${i + 1}: unknown signal "${when[1]}"`); return; }
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

/** Runs parsed events against the simulation clock. */
export class ScenarioRunner {
  private t0 = 0;
  private lastT = 0;
  active = false;
  current = "";

  constructor(private events: ScenarioEvent[], private h: ScenarioHandlers, private read: (signal: string) => number) {}

  start(simTime: number): void {
    this.t0 = simTime;
    this.active = true;
    this.current = "";
    for (const e of this.events) e.fired = false;
  }

  stop(): void { this.active = false; }
  get elapsed(): number { return this.lastT - this.t0; }
  get pendingConditions(): string[] {
    const rel = this.lastT - this.t0;
    return this.events.filter((e) => e.cond && !e.fired && e.t <= rel).map((e) => `${e.cond!.signal} ${e.cond!.op} ${e.cond!.value} → ${e.cmd} ${e.arg}`);
  }

  /** Call every frame with the current simulation time. */
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
    const on = /^(on|1|true|run|deploy)$/i.test(e.arg);
    const num = parseFloat(e.arg);
    this.current = `${e.cond ? "when " + e.cond.signal + e.cond.op + e.cond.value : "t+" + e.t + "s"}: ${e.cmd} ${e.arg}`;
    const n = (d = 0): number => (Number.isFinite(num) ? num : d);
    switch (e.cmd) {
      case "engine": this.h.engine(/^(all|both)$/i.test(e.arg) ? "all" : Math.max(0, n(1) - 1)); break;
      case "starter": this.h.starter(on); break;
      case "fuel": this.h.fuel(on); break;
      case "tla": this.h.tla(n()); break;
      case "n1": this.h.n1(/^off$/i.test(e.arg) ? null : n()); break;
      case "alt": this.h.alt(n()); break;
      case "mach": this.h.mach(n()); break;
      case "isa": this.h.isa(n()); break;
      case "running": this.h.running(n()); break;
      case "antiice": this.h.antiice(on); break;
      case "pack": this.h.pack(on); break;
      case "reverser": this.h.reverser(on); break;
      case "fault": { const [name, ...rest] = e.arg.split(/\s+/); this.h.fault(name, rest.join(" ") || "on"); break; }
      case "bottle": this.h.bottle(); break;
      case "log": this.h.log(on); break;
      case "speed": this.h.speed(n(1)); break;
      case "note": this.h.note(e.arg); break;
      case "end": this.h.end(); this.active = false; break;
    }
  }
}
