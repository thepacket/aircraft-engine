// Canvas strip chart with selectable channel, per-engine series, a reference
// overlay (replayed flight log) and a hover cursor with readouts.
import { SIGNALS, signal, type Signal } from "./signals";
import type { EngineState } from "./types";
import { unitSystem } from "./units";

const ENGINE_COLORS = ["#4fc3f7", "#b388ff"];
const REF_COLOR = "#ffb300";
const CAP = 12000;

class Ring {
  t = new Float64Array(CAP);
  v = new Float64Array(CAP);
  head = 0;
  count = 0;
  push(t: number, v: number): void {
    this.t[this.head] = t;
    this.v[this.head] = v;
    this.head = (this.head + 1) % CAP;
    this.count = Math.min(this.count + 1, CAP);
  }
  clear(): void { this.head = 0; this.count = 0; }
  idx(k: number): number { return (this.head - this.count + k + CAP) % CAP; }
  last(): number { return this.count ? this.v[(this.head - 1 + CAP) % CAP] : NaN; }
  lastT(): number { return this.count ? this.t[(this.head - 1 + CAP) % CAP] : NaN; }
  /** Linear interpolation at time t (NaN outside the data) */
  at(t: number): number {
    if (this.count < 2) return NaN;
    let lo = 0, hi = this.count - 1;
    if (t < this.t[this.idx(lo)] || t > this.t[this.idx(hi)]) return NaN;
    while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (this.t[this.idx(mid)] <= t) lo = mid; else hi = mid; }
    const t0 = this.t[this.idx(lo)], t1 = this.t[this.idx(hi)], v0 = this.v[this.idx(lo)], v1 = this.v[this.idx(hi)];
    if (!Number.isFinite(v0) || !Number.isFinite(v1)) return NaN;
    return t1 > t0 ? v0 + (v1 - v0) * (t - t0) / (t1 - t0) : v0;
  }
}

export class StripChart {
  private ctx: CanvasRenderingContext2D;
  private sig: Signal;
  private series: Ring[] = [];
  private ref = new Ring();
  private hoverX: number | null = null;
  private select: HTMLSelectElement;
  windowS: number;

  constructor(private canvas: HTMLCanvasElement, channel: string, engines: number, windowS = 90, select?: HTMLSelectElement) {
    this.ctx = canvas.getContext("2d")!;
    this.sig = signal(channel);
    this.windowS = windowS;
    this.setEngines(engines);
    this.select = select ?? document.createElement("select");
    for (const s of SIGNALS) {
      const o = document.createElement("option");
      o.value = s.key; o.textContent = s.label;
      this.select.appendChild(o);
    }
    this.select.value = channel;
    this.select.addEventListener("change", () => this.setChannel(this.select.value));
    canvas.addEventListener("mousemove", (e) => { const r = canvas.getBoundingClientRect(); this.hoverX = e.clientX - r.left; this.draw(); });
    canvas.addEventListener("mouseleave", () => { this.hoverX = null; this.draw(); });
  }

  get channel(): string { return this.sig.key; }

  setChannel(key: string): void {
    this.sig = signal(key);
    this.select.value = key;
    for (const r of this.series) r.clear();
    this.ref.clear();
    this.draw();
  }

  setEngines(n: number): void {
    this.series = Array.from({ length: n }, () => new Ring());
  }

  clear(): void { for (const r of this.series) r.clear(); this.ref.clear(); }
  clearRef(): void { this.ref.clear(); }

  push(t: number, states: EngineState[]): void {
    states.forEach((s, i) => { if (this.series[i]) this.series[i].push(t, this.sig.get(s)); });
  }

  /** Reference value (already in SI, the signal's native unit) at sim time t */
  pushRef(t: number, v: number): void { this.ref.push(t, v); }

  private conv(v: number): number { return unitSystem() === "imp" ? this.sig.toImp(v) : v; }
  private unit(): string { return unitSystem() === "imp" ? this.sig.unitImp : this.sig.unitSi; }

  draw(): void {
    const c = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth, h = c.clientHeight;
    if (w === 0 || h === 0) return;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) { c.width = Math.round(w * dpr); c.height = Math.round(h * dpr); }
    const ctx = this.ctx;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const padL = 44, padR = 8, padT = 16, padB = 14;
    const pw = w - padL - padR, ph = h - padT - padB;

    // Range: signal default, expanded to the data
    let vmin = this.conv(this.sig.min), vmax = this.conv(this.sig.max);
    if (vmin > vmax) [vmin, vmax] = [vmax, vmin];
    const all = [...this.series, this.ref];
    const tNow = Math.max(...all.map((r) => (r.count ? r.lastT() : -Infinity)));
    const t0 = tNow - this.windowS;
    for (const r of all) for (let k = 0; k < r.count; k++) {
      const i = r.idx(k);
      if (r.t[i] < t0) continue;
      const v = this.conv(r.v[i]);
      if (Number.isFinite(v)) { if (v < vmin) vmin = v; if (v > vmax) vmax = v; }
    }
    if (vmax - vmin < 1e-9) vmax = vmin + 1;

    ctx.strokeStyle = "#1c2733"; ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) { const y = padT + (ph * i) / 4; ctx.beginPath(); ctx.moveTo(padL, y); ctx.lineTo(w - padR, y); ctx.stroke(); }
    ctx.fillStyle = "#8a9bb0"; ctx.font = "10px ui-monospace, Menlo, monospace"; ctx.textAlign = "right";
    ctx.fillText(fmt(vmax), padL - 3, padT + 4);
    ctx.fillText(fmt((vmax + vmin) / 2), padL - 3, padT + ph / 2 + 3);
    ctx.fillText(fmt(vmin), padL - 3, padT + ph + 3);
    ctx.textAlign = "left";
    const title = `${this.sig.label}${this.unit() ? " (" + this.unit() + ")" : ""}`;
    ctx.fillText(title, padL + 2, 10);
    if (!Number.isFinite(tNow)) return;

    const xOf = (t: number): number => padL + ((t - t0) / this.windowS) * pw;
    const yOf = (v: number): number => padT + ph - ((Math.min(Math.max(v, vmin), vmax) - vmin) / (vmax - vmin)) * ph;
    const plot = (r: Ring, color: string, dash: number[]): void => {
      ctx.strokeStyle = color; ctx.lineWidth = 1.5; ctx.setLineDash(dash); ctx.beginPath();
      let started = false;
      for (let k = 0; k < r.count; k++) {
        const i = r.idx(k);
        if (r.t[i] < t0) continue;
        const v = this.conv(r.v[i]);
        if (!Number.isFinite(v)) { started = false; continue; }
        const x = xOf(r.t[i]), y = yOf(v);
        if (!started) { ctx.moveTo(x, y); started = true; } else ctx.lineTo(x, y);
      }
      ctx.stroke(); ctx.setLineDash([]);
    };
    this.series.forEach((r, i) => plot(r, ENGINE_COLORS[i % ENGINE_COLORS.length], []));
    if (this.ref.count) plot(this.ref, REF_COLOR, [4, 3]);

    // Legend with current (or cursor) values
    let lx = padL + 2 + ctx.measureText(title).width + 14;
    const tCursor = this.hoverX !== null && this.hoverX >= padL && this.hoverX <= w - padR ? t0 + ((this.hoverX - padL) / pw) * this.windowS : null;
    const legend = (name: string, color: string, v: number): void => {
      ctx.fillStyle = color;
      const txt = `${name} ${Number.isFinite(v) ? v.toFixed(this.sig.decimals) : "-"}`;
      ctx.fillText(txt, lx, 10);
      lx += ctx.measureText(txt).width + 12;
    };
    this.series.forEach((r, i) => legend(this.series.length > 1 ? `E${i + 1}` : "sim", ENGINE_COLORS[i % ENGINE_COLORS.length], this.conv(tCursor !== null ? r.at(tCursor) : r.last())));
    if (this.ref.count) legend("log", REF_COLOR, this.conv(tCursor !== null ? this.ref.at(tCursor) : this.ref.last()));

    if (tCursor !== null) {
      ctx.strokeStyle = "#5f7187"; ctx.setLineDash([2, 3]); ctx.beginPath(); ctx.moveTo(this.hoverX!, padT); ctx.lineTo(this.hoverX!, padT + ph); ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = "#8a9bb0"; ctx.textAlign = "center"; ctx.fillText(`t = ${tCursor.toFixed(1)} s`, this.hoverX!, h - 2);
    } else {
      ctx.fillStyle = "#5f7187"; ctx.textAlign = "left"; ctx.fillText(`t = ${tNow.toFixed(0)} s`, padL + 2, h - 2);
    }
  }
}

function fmt(v: number): string {
  const a = Math.abs(v);
  return a >= 100 ? v.toFixed(0) : a >= 10 ? v.toFixed(1) : v.toFixed(2);
}
