// EICAS-style round dials drawn in SVG (Boeing 737NG display conventions:
// white scale and pointer, digital readout in a box, red radial at the limit,
// amber caution band).

const NS = "http://www.w3.org/2000/svg";

export interface DialOptions {
  label: string;
  unit?: string;
  min: number;
  max: number;
  /** Red line (limit) value */
  redline?: number;
  /** Amber band [from, to] */
  amber?: [number, number];
  /** Green band [from, to] (oil pressure normal range etc.) */
  green?: [number, number];
  decimals?: number;
  /** Digital-only (no pointer) e.g. fuel flow */
  digitalOnly?: boolean;
  /** Reference bug (e.g. commanded N1) */
  bug?: boolean;
  /** Low limit red line (oil pressure minimum) */
  redlineLow?: number;
}

const START_DEG = -210; // scale start angle (degrees, 0 = 3 o'clock, clockwise)
const SWEEP_DEG = 240;

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number>): SVGElementTagNameMap[K] {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const a = (deg * Math.PI) / 180;
  return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
}

function arcPath(cx: number, cy: number, r: number, a0: number, a1: number): string {
  const [x0, y0] = polar(cx, cy, r, a0);
  const [x1, y1] = polar(cx, cy, r, a1);
  const large = a1 - a0 > 180 ? 1 : 0;
  return `M ${x0.toFixed(2)} ${y0.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
}

export class Dial {
  private svg: SVGSVGElement;
  private pointer?: SVGLineElement;
  private bugEl?: SVGPolygonElement;
  private valueText: SVGTextElement;
  private valueBox: SVGRectElement;
  private readonly cx = 100;
  private readonly cy = 95;
  private readonly r = 62;
  private opts: DialOptions;

  constructor(container: HTMLElement, opts: DialOptions) {
    this.opts = opts;
    const svg = el("svg", { viewBox: "0 0 200 170" });
    this.svg = svg;
    container.appendChild(svg);
    const { cx, cy, r } = this;

    if (!opts.digitalOnly) {
      // Scale arc
      svg.appendChild(el("path", { d: arcPath(cx, cy, r, START_DEG, START_DEG + SWEEP_DEG), stroke: "#e8eef4", "stroke-width": 2, fill: "none" }));
      // Bands
      if (opts.green) this.band(opts.green[0], opts.green[1], "#4caf50", 5);
      if (opts.amber) this.band(opts.amber[0], opts.amber[1], "#ffb300", 5);
      // Ticks and labels
      const n = 10;
      for (let i = 0; i <= n; i++) {
        const v = opts.min + ((opts.max - opts.min) * i) / n;
        const a = this.angle(v);
        const major = i % 2 === 0;
        const [x0, y0] = polar(cx, cy, r, a);
        const [x1, y1] = polar(cx, cy, r - (major ? 9 : 5), a);
        svg.appendChild(el("line", { x1: x0, y1: y0, x2: x1, y2: y1, stroke: "#e8eef4", "stroke-width": major ? 2 : 1 }));
        if (major) {
          const [tx, ty] = polar(cx, cy, r - 19, a);
          const t = el("text", { x: tx, y: ty + 3.5, "text-anchor": "middle", fill: "#cfd8e2", "font-size": 9, "font-family": "ui-monospace, Menlo, monospace" });
          t.textContent = String(Math.round(v));
          svg.appendChild(t);
        }
      }
      // Red lines
      if (opts.redline !== undefined) this.redline(opts.redline);
      if (opts.redlineLow !== undefined) this.redline(opts.redlineLow);
      // Pointer
      this.pointer = el("line", { x1: cx, y1: cy, x2: cx, y2: cy - r + 2, stroke: "#ffffff", "stroke-width": 3, "stroke-linecap": "round" });
      svg.appendChild(this.pointer);
      svg.appendChild(el("circle", { cx, cy, r: 4, fill: "#ffffff" }));
      if (opts.bug) {
        this.bugEl = el("polygon", { points: "0,0 -5,-9 5,-9", fill: "#b388ff" });
        svg.appendChild(this.bugEl);
      }
    }
    // Digital readout box
    const bw = opts.digitalOnly ? 96 : 76;
    this.valueBox = el("rect", { x: cx - bw / 2, y: opts.digitalOnly ? cy - 30 : cy + 18, width: bw, height: 24, fill: "#000", stroke: "#8a9bb0", "stroke-width": 1, rx: 2 });
    svg.appendChild(this.valueBox);
    this.valueText = el("text", { x: cx + bw / 2 - 6, y: opts.digitalOnly ? cy - 12 : cy + 36, "text-anchor": "end", fill: "#ffffff", "font-size": 18, "font-family": "ui-monospace, Menlo, monospace", "font-weight": 600 });
    this.valueText.textContent = "---";
    svg.appendChild(this.valueText);
    // Label
    const label = el("text", { x: cx, y: 160, "text-anchor": "middle", fill: "#4fc3f7", "font-size": 12, "font-family": "-apple-system, Helvetica, Arial, sans-serif", "font-weight": 600 });
    label.textContent = opts.unit ? `${opts.label}  ${opts.unit}` : opts.label;
    svg.appendChild(label);
  }

  private angle(v: number): number {
    const f = (Math.min(Math.max(v, this.opts.min), this.opts.max) - this.opts.min) / (this.opts.max - this.opts.min);
    return START_DEG + SWEEP_DEG * f;
  }

  private band(from: number, to: number, color: string, width: number): void {
    const a0 = this.angle(from);
    const a1 = this.angle(to);
    this.svg.appendChild(el("path", { d: arcPath(this.cx, this.cy, this.r + 4, a0, a1), stroke: color, "stroke-width": width, fill: "none" }));
  }

  private redline(v: number): void {
    const a = this.angle(v);
    const [x0, y0] = polar(this.cx, this.cy, this.r - 2, a);
    const [x1, y1] = polar(this.cx, this.cy, this.r + 9, a);
    this.svg.appendChild(el("line", { x1: x0, y1: y0, x2: x1, y2: y1, stroke: "#ff3d3d", "stroke-width": 3.5 }));
  }

  set(value: number, bug?: number): void {
    const d = this.opts.decimals ?? 0;
    this.valueText.textContent = Number.isFinite(value) ? value.toFixed(d) : "---";
    if (this.pointer) {
      const a = this.angle(value);
      const [x, y] = polar(this.cx, this.cy, this.r - 2, a);
      this.pointer.setAttribute("x2", x.toFixed(2));
      this.pointer.setAttribute("y2", y.toFixed(2));
    }
    if (this.bugEl && bug !== undefined) {
      const a = this.angle(bug);
      const [x, y] = polar(this.cx, this.cy, this.r + 6, a);
      this.bugEl.setAttribute("transform", `translate(${x.toFixed(2)},${y.toFixed(2)}) rotate(${(a + 90).toFixed(1)})`);
    }
    // Exceedance colouring of the readout
    let color = "#ffffff";
    const o = this.opts;
    if (o.redline !== undefined && value > o.redline) color = "#ff3d3d";
    else if (o.redlineLow !== undefined && value < o.redlineLow) color = "#ff3d3d";
    else if (o.amber && value >= o.amber[0] && value <= o.amber[1]) color = "#ffb300";
    this.valueText.setAttribute("fill", color);
    this.valueBox.setAttribute("stroke", color === "#ffffff" ? "#8a9bb0" : color);
  }
}
