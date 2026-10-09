// Flight-log import, column mapping, replay and model-vs-log comparison.
//
// A log is any CSV with a time column. Input columns drive the simulator
// (main lever, N1 target, fuel lever, starter, altitude, Mach, ISA deviation);
// measured columns are compared against the active engine kind's signals.
import { activeSignals, type Signal } from "./signals";

export interface LogTable { columns: string[]; rows: Float64Array[]; n: number }

export function parseCsv(text: string): LogTable {
  const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0);
  if (lines.length < 2) throw new Error("CSV needs a header and at least one row");
  const sep = lines[0].includes(";") && !lines[0].includes(",") ? ";" : lines[0].includes("\t") ? "\t" : ",";
  const columns = lines[0].split(sep).map((c) => c.trim().replace(/^"|"$/g, ""));
  const rows: Float64Array[] = columns.map(() => new Float64Array(lines.length - 1));
  let n = 0;
  for (let i = 1; i < lines.length; i++) {
    const cells = lines[i].split(sep);
    if (cells.length < 2) continue;
    for (let c = 0; c < columns.length; c++) {
      const v = parseFloat(cells[c]);
      rows[c][n] = Number.isFinite(v) ? v : NaN;
    }
    n++;
  }
  return { columns, rows: rows.map((r) => r.subarray(0, n)), n };
}

export const INPUT_CHANNELS = ["time", "lever", "n1_cmd", "fuel_lever", "starter", "alt", "mach", "isa"] as const;
export type InputChannel = (typeof INPUT_CHANNELS)[number];

export interface Mapping {
  /** input channel or signal key -> column name */
  columns: Record<string, string | undefined>;
  /** unit option index per channel (inputs: lever/alt/time; signals: importUnits index) */
  units: Record<string, number>;
}

const INPUT_PATTERNS: Record<InputChannel, RegExp[]> = {
  time: [/^time_s$/i, /^time$/i, /^t$/i, /^sec/i, /^timestamp/i, /^elapsed/i],
  lever: [/^tla$/i, /^pla/i, /thrust.?lever/i, /^throttle/i, /^power.?lever/i, /^tra$/i],
  n1_cmd: [/n1.?cmd/i, /n1.?target/i, /n1.?demand/i, /n1.?ref/i],
  fuel_lever: [/fuel.?lever/i, /start.?lever/i, /fuel.?switch/i, /^cutoff/i, /condition/i, /^mixture/i],
  starter: [/^starter$/i, /start.?valve/i],
  alt: [/^altitude_m$/i, /^alt/i, /baro/i, /^h$/i],
  mach: [/^mach/i, /^m$/i],
  isa: [/isa/i, /^delta_isa/i],
};
export const INPUT_UNITS: Partial<Record<InputChannel, { label: string; toSi: (v: number) => number }[]>> = {
  time: [{ label: "s", toSi: (v) => v }, { label: "ms", toSi: (v) => v / 1000 }],
  lever: [{ label: "0..1", toSi: (v) => v }, { label: "%", toSi: (v) => v / 100 }, { label: "deg (0..50)", toSi: (v) => Math.min(Math.max(v / 50, 0), 1) }],
  alt: [{ label: "m", toSi: (v) => v }, { label: "ft", toSi: (v) => v * 0.3048 }],
  mach: [{ label: "Mach", toSi: (v) => v }, { label: "kt TAS (SL)", toSi: (v) => (v * 0.514444) / 340.3 }],
};

export function measuredSignals(): Signal[] {
  return activeSignals().filter((s) => s.patterns && s.patterns.length);
}

export function autoMap(columns: string[]): Mapping {
  const m: Mapping = { columns: {}, units: {} };
  const lower = (c: string): string => c.toLowerCase();
  for (const ch of INPUT_CHANNELS) {
    for (const re of INPUT_PATTERNS[ch]) { const hit = columns.find((c) => re.test(c)); if (hit) { m.columns[ch] = hit; break; } }
    m.units[ch] = 0;
  }
  const used = new Set(Object.values(m.columns));
  for (const s of measuredSignals()) {
    for (const re of s.patterns!) { const hit = columns.find((c) => !used.has(c) && re.test(c)); if (hit) { m.columns[s.key] = hit; used.add(hit); break; } }
    m.units[s.key] = 0;
    const name = lower(m.columns[s.key] ?? "");
    const opts = s.importUnits ?? [];
    const pick = (re: RegExp): void => { const i = opts.findIndex((u) => re.test(u.label.toLowerCase())); if (i >= 0) m.units[s.key] = i; };
    if (/pph|lb.?h|lbs.?hr/.test(name)) pick(/lb\/h/);
    else if (/gph|gal/.test(name)) pick(/gal/);
    else if (/kg.?h/.test(name)) pick(/kg\/h/);
    if (/_f$|fahr|°f/.test(name)) pick(/°f/);
    else if (/_k$|kelvin/.test(name)) pick(/^k$/);
    else if (/_c$|celsius|°c/.test(name)) pick(/°c/);
    if (/lbf/.test(name)) pick(/lbf/);
    if (/n.?m$/.test(name) && s.key === "torque") pick(/n·m/);
  }
  if (/ft|feet/.test(lower(m.columns.alt ?? ""))) m.units.alt = 1;
  if (/pct|%/.test(lower(m.columns.lever ?? ""))) m.units.lever = 1;
  if (/ms/.test(lower(m.columns.time ?? ""))) m.units.time = 1;
  return m;
}

export interface ReplaySample {
  t: number;
  lever?: number;
  n1_cmd?: number;
  fuel_lever?: boolean;
  starter?: boolean;
  alt?: number;
  mach?: number;
  isa?: number;
  measured: Record<string, number>;
}

export class Replay {
  private t: Float64Array;
  readonly duration: number;
  private cols: Map<string, Float64Array>;
  private sigs: Signal[];

  constructor(table: LogTable, private map: Mapping) {
    const tc = map.columns.time;
    if (!tc) throw new Error("no time column mapped");
    const raw = table.rows[table.columns.indexOf(tc)];
    const u = INPUT_UNITS.time![map.units.time ?? 0];
    const t0 = u.toSi(raw[0]);
    this.t = Float64Array.from(raw, (v) => u.toSi(v) - t0);
    this.duration = this.t[this.t.length - 1];
    this.cols = new Map(table.columns.map((c, i) => [c, table.rows[i]]));
    this.sigs = measuredSignals();
  }

  private col(ch: string): Float64Array | undefined {
    const name = this.map.columns[ch];
    return name ? this.cols.get(name) : undefined;
  }

  private interp(arr: Float64Array, t: number): number {
    const n = this.t.length;
    if (t <= this.t[0]) return arr[0];
    if (t >= this.t[n - 1]) return arr[n - 1];
    let lo = 0, hi = n - 1;
    while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (this.t[mid] <= t) lo = mid; else hi = mid; }
    const f = (t - this.t[lo]) / Math.max(this.t[hi] - this.t[lo], 1e-9);
    const a = arr[lo], b = arr[hi];
    if (!Number.isFinite(a)) return b;
    if (!Number.isFinite(b)) return a;
    return a + (b - a) * f;
  }

  sample(t: number): ReplaySample {
    const s: ReplaySample = { t, measured: {} };
    const input = (ch: InputChannel): number | undefined => {
      const c = this.col(ch); if (!c) return undefined;
      const units = INPUT_UNITS[ch]; const v = this.interp(c, t);
      return units ? units[this.map.units[ch] ?? 0].toSi(v) : v;
    };
    s.lever = input("lever"); s.n1_cmd = input("n1_cmd"); s.alt = input("alt"); s.mach = input("mach"); s.isa = input("isa");
    const fl = input("fuel_lever"), st = input("starter");
    for (const sg of this.sigs) {
      const c = this.col(sg.key); if (!c) continue;
      const v = this.interp(c, t); if (!Number.isFinite(v)) continue;
      const u = sg.importUnits?.[this.map.units[sg.key] ?? 0];
      s.measured[sg.key] = u ? u.toSi(v) : v;
    }
    // Lever inference when not logged: fuel on while fuel is flowing or the
    // core turns above idle; starter while the core is below light-off with no fuel.
    const ff = s.measured.ff, core = s.measured.n2 ?? s.measured.ng ?? s.measured.rpm;
    const coreIdle = s.measured.rpm !== undefined ? 400 : 30;
    s.fuel_lever = fl !== undefined ? fl > 0.5 : ff !== undefined ? ff > 0.0005 : core !== undefined ? core > coreIdle : true;
    s.starter = st !== undefined ? st > 0.5 : core !== undefined ? core > 0.5 && core < coreIdle * 1.6 && !(ff !== undefined && ff > 0.0005 && core > coreIdle * 1.5) : false;
    if (s.lever === undefined && s.n1_cmd === undefined && s.measured.n1 !== undefined) s.n1_cmd = s.measured.n1;
    return s;
  }
}

export interface ChannelStats { n: number; rms: number; mean: number; max: number; meanAbs: number }

export class Comparison {
  private sums = new Map<string, { n: number; se: number; e: number; ae: number; max: number }>();
  add(key: string, sim: number, meas: number): void {
    if (!Number.isFinite(sim) || !Number.isFinite(meas)) return;
    const e = sim - meas;
    const s = this.sums.get(key) ?? { n: 0, se: 0, e: 0, ae: 0, max: 0 };
    s.n++; s.se += e * e; s.e += e; s.ae += Math.abs(e); s.max = Math.max(s.max, Math.abs(e));
    this.sums.set(key, s);
  }
  stats(): Record<string, ChannelStats> {
    const out: Record<string, ChannelStats> = {};
    for (const [k, s] of this.sums) out[k] = { n: s.n, rms: Math.sqrt(s.se / s.n), mean: s.e / s.n, max: s.max, meanAbs: s.ae / s.n };
    return out;
  }
  clear(): void { this.sums.clear(); }
}
