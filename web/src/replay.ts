// Flight-log import, column mapping, replay and model-vs-log comparison.
//
// A log is any CSV with a time column. Columns are mapped (automatically by
// name, then adjustable) onto the simulator's inputs (thrust lever or N1,
// fuel lever, starter, altitude, Mach, ISA deviation) and onto measured
// channels to compare against (N1, N2, EGT, fuel flow, oil, ...).

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

/** Simulator input channels and measured channels a log can provide. */
export const INPUT_CHANNELS = ["time", "tla", "n1_cmd", "fuel_lever", "starter", "alt", "mach", "isa"] as const;
export const MEASURED_CHANNELS = ["n1", "n2", "egt", "ff", "thrust", "oilp", "oilt", "vib", "t4"] as const;
export type InputChannel = (typeof INPUT_CHANNELS)[number];
export type MeasuredChannel = (typeof MEASURED_CHANNELS)[number];

export interface Mapping {
  columns: Partial<Record<InputChannel | MeasuredChannel, string>>;
  /** unit conversions into SI */
  tlaScale: "frac" | "pct" | "deg";
  altUnit: "m" | "ft";
  ffUnit: "kg_s" | "kg_h" | "pph";
  thrustUnit: "n" | "kn" | "lbf";
  timeUnit: "s" | "ms";
  egtUnit: "c" | "k" | "f";
}

const PATTERNS: Record<InputChannel | MeasuredChannel, RegExp[]> = {
  time: [/^time_s$/i, /^time$/i, /^t$/i, /^sec/i, /^timestamp/i, /^elapsed/i],
  tla: [/^tla$/i, /^pla/i, /thrust.?lever/i, /^throttle/i, /^tra$/i],
  n1_cmd: [/n1.?cmd/i, /n1.?target/i, /n1.?demand/i, /n1.?ref/i],
  fuel_lever: [/fuel.?lever/i, /start.?lever/i, /fuel.?switch/i, /^cutoff/i],
  starter: [/^starter$/i, /start.?valve/i],
  alt: [/^altitude_m$/i, /^alt/i, /baro/i, /^h$/i],
  mach: [/^mach/i, /^m$/i],
  isa: [/isa/i, /^delta_isa/i],
  n1: [/^n1_pct$/i, /^n1$/i, /^n1[^c_]/i, /^n1\.1$/i, /^n1\b/i],
  n2: [/^n2_pct$/i, /^n2$/i, /^n2\.1$/i, /^n2\b/i],
  egt: [/^egt_c$/i, /^egt$/i, /^egt\.1$/i, /^egt\b/i, /^itt/i],
  ff: [/^fuel_flow_kg_s$/i, /^ff$/i, /^ff\.1$/i, /fuel.?flow/i, /^wf/i],
  thrust: [/^thrust_n$/i, /^thrust/i, /^fn$/i],
  oilp: [/oil.?press/i, /^oilp/i],
  oilt: [/oil.?temp/i, /^oilt/i],
  vib: [/^vib/i, /vibration/i],
  t4: [/^t4_k$/i, /^t4$/i, /^tit/i],
};

export function autoMap(columns: string[]): Mapping {
  const m: Mapping = { columns: {}, tlaScale: "frac", altUnit: "m", ffUnit: "kg_s", thrustUnit: "n", timeUnit: "s", egtUnit: "c" };
  for (const ch of [...INPUT_CHANNELS, ...MEASURED_CHANNELS]) {
    for (const re of PATTERNS[ch]) {
      const hit = columns.find((c) => re.test(c));
      if (hit) { m.columns[ch] = hit; break; }
    }
  }
  // Unit guesses from the column names
  const name = (ch: InputChannel | MeasuredChannel): string => (m.columns[ch] ?? "").toLowerCase();
  if (/ft|feet/.test(name("alt"))) m.altUnit = "ft";
  if (/pph|lb.?h|lbs.?hr/.test(name("ff"))) m.ffUnit = "pph";
  else if (/kg.?h/.test(name("ff"))) m.ffUnit = "kg_h";
  if (/lbf|lb/.test(name("thrust"))) m.thrustUnit = "lbf";
  else if (/kn/.test(name("thrust"))) m.thrustUnit = "kn";
  if (/pct|%/.test(name("tla"))) m.tlaScale = "pct";
  else if (/deg|pla/.test(name("tla"))) m.tlaScale = "deg";
  if (/ms/.test(name("time"))) m.timeUnit = "ms";
  if (/_k$|kelvin/.test(name("egt"))) m.egtUnit = "k";
  else if (/_f$|fahr/.test(name("egt"))) m.egtUnit = "f";
  return m;
}

export interface ReplaySample {
  t: number;
  tla?: number;
  n1_cmd?: number;
  fuel_lever?: boolean;
  starter?: boolean;
  alt?: number;
  mach?: number;
  isa?: number;
  measured: Partial<Record<MeasuredChannel, number>>;
}

/** Time-indexed access to a mapped log, with inference of the levers when the
 *  log does not record them. */
export class Replay {
  private t: Float64Array;
  readonly duration: number;
  private cols: Map<string, Float64Array>;

  constructor(table: LogTable, private map: Mapping) {
    const tc = map.columns.time;
    if (!tc) throw new Error("no time column mapped");
    const raw = table.rows[table.columns.indexOf(tc)];
    const scale = map.timeUnit === "ms" ? 0.001 : 1;
    const t0 = raw[0] * scale;
    this.t = Float64Array.from(raw, (v) => v * scale - t0);
    this.duration = this.t[this.t.length - 1];
    this.cols = new Map(table.columns.map((c, i) => [c, table.rows[i]]));
  }

  private col(ch: InputChannel | MeasuredChannel): Float64Array | undefined {
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

  private toSi(ch: MeasuredChannel | InputChannel, v: number): number {
    const m = this.map;
    switch (ch) {
      case "tla": return m.tlaScale === "pct" ? v / 100 : m.tlaScale === "deg" ? Math.min(Math.max(v / 50, 0), 1) : v;
      case "alt": return m.altUnit === "ft" ? v * 0.3048 : v;
      case "ff": return m.ffUnit === "pph" ? v / 7936.64 : m.ffUnit === "kg_h" ? v / 3600 : v;
      case "thrust": return m.thrustUnit === "lbf" ? v * 4.448222 : m.thrustUnit === "kn" ? v * 1000 : v;
      case "egt": return m.egtUnit === "k" ? v - 273.15 : m.egtUnit === "f" ? (v - 32) / 1.8 : v;
      default: return v;
    }
  }

  sample(t: number): ReplaySample {
    const s: ReplaySample = { t, measured: {} };
    const get = (ch: InputChannel | MeasuredChannel): number | undefined => { const c = this.col(ch); return c ? this.toSi(ch, this.interp(c, t)) : undefined; };
    s.tla = get("tla");
    s.n1_cmd = get("n1_cmd");
    s.alt = get("alt");
    s.mach = get("mach");
    s.isa = get("isa");
    const fl = get("fuel_lever"), st = get("starter");
    for (const ch of MEASURED_CHANNELS) { const v = get(ch); if (v !== undefined && Number.isFinite(v)) s.measured[ch] = v; }
    // Inference when the levers are not logged: fuel on while fuel is flowing
    // or the core is above idle; starter while the core is below light-off
    // speed with no fuel.
    const ff = s.measured.ff, n2 = s.measured.n2;
    s.fuel_lever = fl !== undefined ? fl > 0.5 : ff !== undefined ? ff > 0.005 : n2 !== undefined ? n2 > 30 : true;
    s.starter = st !== undefined ? st > 0.5 : n2 !== undefined ? n2 > 0.5 && n2 < 50 && !(ff !== undefined && ff > 0.005 && n2 > 45) : false;
    // No lever data at all: follow logged N1
    if (s.tla === undefined && s.n1_cmd === undefined && s.measured.n1 !== undefined) s.n1_cmd = s.measured.n1;
    return s;
  }
}

export interface ChannelStats { n: number; rms: number; mean: number; max: number; meanAbs: number }

/** Accumulates sim-vs-log differences per channel. */
export class Comparison {
  private sums = new Map<MeasuredChannel, { n: number; se: number; e: number; ae: number; max: number }>();

  add(ch: MeasuredChannel, sim: number, meas: number): void {
    if (!Number.isFinite(sim) || !Number.isFinite(meas)) return;
    const e = sim - meas;
    const s = this.sums.get(ch) ?? { n: 0, se: 0, e: 0, ae: 0, max: 0 };
    s.n++; s.se += e * e; s.e += e; s.ae += Math.abs(e); s.max = Math.max(s.max, Math.abs(e));
    this.sums.set(ch, s);
  }

  stats(): Partial<Record<MeasuredChannel, ChannelStats>> {
    const out: Partial<Record<MeasuredChannel, ChannelStats>> = {};
    for (const [ch, s] of this.sums) out[ch] = { n: s.n, rms: Math.sqrt(s.se / s.n), mean: s.e / s.n, max: s.max, meanAbs: s.ae / s.n };
    return out;
  }

  clear(): void { this.sums.clear(); }
}
