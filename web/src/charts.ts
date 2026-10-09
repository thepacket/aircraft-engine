// Lightweight canvas strip chart: fixed time window, multiple series.

export interface Series {
  name: string;
  color: string;
  /** optional separate y-range when the series uses the right axis */
  min: number;
  max: number;
}

export class StripChart {
  private ctx: CanvasRenderingContext2D;
  private data: Map<string, Float64Array>;
  private times: Float64Array;
  private head = 0;
  private count = 0;
  private readonly cap = 6000;

  constructor(private canvas: HTMLCanvasElement, private series: Series[], private windowS = 90, private title = "") {
    this.ctx = canvas.getContext("2d")!;
    this.data = new Map(series.map((s) => [s.name, new Float64Array(this.cap)]));
    this.times = new Float64Array(this.cap);
  }

  clear(): void {
    this.head = 0;
    this.count = 0;
  }

  push(t: number, values: Record<string, number>): void {
    const i = this.head;
    this.times[i] = t;
    for (const s of this.series) this.data.get(s.name)![i] = values[s.name] ?? NaN;
    this.head = (i + 1) % this.cap;
    this.count = Math.min(this.count + 1, this.cap);
  }

  draw(): void {
    const c = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    if (w === 0 || h === 0) return;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
    }
    const ctx = this.ctx;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const padL = 36, padR = 8, padT = 14, padB = 14;
    const pw = w - padL - padR;
    const ph = h - padT - padB;

    // Grid
    ctx.strokeStyle = "#1c2733";
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
      const y = padT + (ph * i) / 4;
      ctx.beginPath();
      ctx.moveTo(padL, y);
      ctx.lineTo(w - padR, y);
      ctx.stroke();
    }
    ctx.fillStyle = "#8a9bb0";
    ctx.font = "10px ui-monospace, Menlo, monospace";
    ctx.textAlign = "left";
    ctx.fillText(this.title, padL + 2, 10);

    if (this.count === 0) return;
    const tNow = this.times[(this.head - 1 + this.cap) % this.cap];
    const t0 = tNow - this.windowS;

    // Axis labels from the first series range
    const s0 = this.series[0];
    ctx.textAlign = "right";
    ctx.fillText(s0.max.toFixed(0), padL - 3, padT + 4);
    ctx.fillText(s0.min.toFixed(0), padL - 3, padT + ph + 3);

    let lx = padL + 2 + ctx.measureText(this.title).width + 14;
    for (const s of this.series) {
      const arr = this.data.get(s.name)!;
      ctx.strokeStyle = s.color;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      let started = false;
      for (let k = 0; k < this.count; k++) {
        const idx = (this.head - this.count + k + this.cap) % this.cap;
        const t = this.times[idx];
        if (t < t0) continue;
        const v = arr[idx];
        if (!Number.isFinite(v)) { started = false; continue; }
        const x = padL + ((t - t0) / this.windowS) * pw;
        const y = padT + ph - ((Math.min(Math.max(v, s.min), s.max) - s.min) / (s.max - s.min)) * ph;
        if (!started) { ctx.moveTo(x, y); started = true; } else ctx.lineTo(x, y);
      }
      ctx.stroke();
      // legend
      ctx.fillStyle = s.color;
      ctx.textAlign = "left";
      const last = arr[(this.head - 1 + this.cap) % this.cap];
      const txt = `${s.name} ${Number.isFinite(last) ? last.toFixed(s.max - s.min > 50 ? 0 : 2) : "-"}`;
      ctx.fillText(txt, lx, 10);
      lx += ctx.measureText(txt).width + 12;
    }
  }
}
