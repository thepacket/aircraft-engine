// Scenario scripts: `at <seconds> <command> [value...]`, one event per line.
// Same syntax as the native engine-cli so a lesson can be run in either.

export interface ScenarioEvent {
  t: number;
  cmd: string;
  arg: string;
  line: number;
}

export interface ScenarioHandlers {
  starter(on: boolean): void;
  fuel(on: boolean): void;
  tla(v: number): void;
  alt(m: number): void;
  mach(v: number): void;
  isa(dK: number): void;
  running(tla: number): void;
  log(on: boolean): void;
  speed(x: number): void;
  note(text: string): void;
  end(): void;
}

const COMMANDS = new Set(["starter", "fuel", "tla", "alt", "mach", "isa", "running", "log", "speed", "note", "end"]);

export function parseScenario(text: string): { events: ScenarioEvent[]; errors: string[] } {
  const events: ScenarioEvent[] = [];
  const errors: string[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.split("#")[0].trim();
    if (!line) return;
    const m = line.match(/^at\s+([0-9.]+)\s+(\S+)\s*(.*)$/);
    if (!m) {
      errors.push(`line ${i + 1}: expected "at <seconds> <command> [value]"`);
      return;
    }
    const cmd = m[2].toLowerCase();
    if (!COMMANDS.has(cmd)) {
      errors.push(`line ${i + 1}: unknown command "${m[2]}"`);
      return;
    }
    events.push({ t: parseFloat(m[1]), cmd, arg: m[3].trim(), line: i + 1 });
  });
  events.sort((a, b) => a.t - b.t || a.line - b.line);
  return { events, errors };
}

/** Runs parsed events against the simulation clock. */
export class ScenarioRunner {
  private next = 0;
  private t0 = 0;
  active = false;
  current = "";

  constructor(private events: ScenarioEvent[], private h: ScenarioHandlers) {}

  start(simTime: number): void {
    this.t0 = simTime;
    this.next = 0;
    this.active = true;
    this.current = "";
  }

  stop(): void {
    this.active = false;
  }

  get elapsed(): number {
    return this.lastT - this.t0;
  }
  private lastT = 0;

  /** Call every frame with the current simulation time. */
  tick(simTime: number): void {
    if (!this.active) return;
    this.lastT = simTime;
    const rel = simTime - this.t0;
    while (this.next < this.events.length && this.events[this.next].t <= rel + 1e-9) {
      const e = this.events[this.next++];
      this.apply(e);
      if (!this.active) return;
    }
    if (this.next >= this.events.length) this.active = false;
  }

  private apply(e: ScenarioEvent): void {
    const on = /^(on|1|true|run)$/i.test(e.arg);
    const num = parseFloat(e.arg);
    this.current = `t+${e.t}s: ${e.cmd} ${e.arg}`;
    switch (e.cmd) {
      case "starter": this.h.starter(on); break;
      case "fuel": this.h.fuel(on); break;
      case "tla": this.h.tla(Number.isFinite(num) ? num : 0); break;
      case "alt": this.h.alt(Number.isFinite(num) ? num : 0); break;
      case "mach": this.h.mach(Number.isFinite(num) ? num : 0); break;
      case "isa": this.h.isa(Number.isFinite(num) ? num : 0); break;
      case "running": this.h.running(Number.isFinite(num) ? num : 0); break;
      case "log": this.h.log(on); break;
      case "speed": this.h.speed(Number.isFinite(num) ? num : 1); break;
      case "note": this.h.note(e.arg); break;
      case "end": this.h.end(); this.active = false; break;
    }
  }
}
