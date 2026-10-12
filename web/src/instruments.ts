// EICAS-style round dials drawn in SVG (Boeing 737NG display unit
// conventions): round black instrument face inside a dark bezel, grey scale
// arc, white ticks and pointer, a white arc that fills from the scale start
// to the pointer, a boxed digital readout, red radial at the limit, amber
// caution band, magenta reference bug.

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

/** Width / height of one dial drawing (its SVG viewBox) */
export const DIAL_ASPECT = 200 / 176;

/**
 * Column count that maximises the size of `n` units of aspect ratio `aspect`
 * (width / height) laid out in a `w`×`h` grid with `gap` between cells.
 */
export function bestColumns(n: number, aspect: number, w: number, h: number, gap: number): number {
  let best = 1, bestSize = -1;
  for (let c = 1; c <= n; c++) {
    const r = Math.ceil(n / c);
    const cellW = (w - gap * (c - 1)) / c;
    const cellH = (h - gap * (r - 1)) / r;
    const size = Math.min(cellW, cellH * aspect); // unit width that fits the cell
    if (size > bestSize + 0.5) { best = c; bestSize = size; }
  }
  return best;
}

const START_DEG = -210; // scale start angle (degrees, 0 = 3 o'clock, clockwise)
const SWEEP_DEG = 240;

const C = {
  white: "#f4f7fa",
  scale: "#8e98a4",
  tick: "#dfe6ee",
  label: "#19d3f5",
  amber: "#ffb300",
  red: "#ff3d3d",
  green: "#39d353",
  magenta: "#ff4fd8",
  face: "#040608",
  bezel: "#1a1e24",
  rim: "#2d333b",
};

let gradientCounter = 0;

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
  private pointer?: SVGPolygonElement;
  private fillArc?: SVGPathElement;
  private bugEl?: SVGPolygonElement;
  private valueText: SVGTextElement;
  private valueBox: SVGRectElement;
  private readonly cx = 100;
  private readonly cy = 92;
  private readonly r = 60;
  private opts: DialOptions;

  constructor(container: HTMLElement, opts: DialOptions) {
    this.opts = opts;
    const svg = el("svg", { viewBox: "0 0 200 176", class: "dial-svg" });
    this.svg = svg;
    container.appendChild(svg);
    const { cx, cy, r } = this;

    // Bezel and face
    const gid = `dial-face-${gradientCounter++}`;
    const defs = el("defs", {});
    const grad = el("radialGradient", { id: gid, cx: "50%", cy: "45%", r: "60%" });
    grad.appendChild(el("stop", { offset: "0%", "stop-color": "#0d1115" }));
    grad.appendChild(el("stop", { offset: "100%", "stop-color": C.face }));
    defs.appendChild(grad);
    svg.appendChild(defs);
    const bezelR = r + 20;
    svg.appendChild(el("circle", { cx, cy, r: bezelR + 2, fill: C.rim }));
    svg.appendChild(el("circle", { cx, cy, r: bezelR, fill: C.bezel }));
    svg.appendChild(el("circle", { cx, cy, r: bezelR - 4, fill: `url(#${gid})`, stroke: "#000", "stroke-width": 1 }));
    // Bezel screws
    for (const a of [-135, -45, 45, 135]) {
      const [sx, sy] = polar(cx, cy, bezelR - 1.5, a);
      svg.appendChild(el("circle", { cx: sx, cy: sy, r: 1.6, fill: "#3b424b" }));
    }

    if (!opts.digitalOnly) {
      // Scale arc
      svg.appendChild(el("path", { d: arcPath(cx, cy, r, START_DEG, START_DEG + SWEEP_DEG), stroke: C.scale, "stroke-width": 2, fill: "none" }));
      // Bands (outside the scale)
      if (opts.green) this.band(opts.green[0], opts.green[1], C.green, 4);
      if (opts.amber) this.band(opts.amber[0], opts.amber[1], C.amber, 4);
      // Fill arc from the scale start to the pointer
      this.fillArc = el("path", { d: "", stroke: C.white, "stroke-width": 3.5, fill: "none", "stroke-linecap": "butt" });
      svg.appendChild(this.fillArc);
      // Ticks and labels
      const n = 10;
      for (let i = 0; i <= n; i++) {
        const v = opts.min + ((opts.max - opts.min) * i) / n;
        const a = this.angle(v);
        const major = i % 2 === 0;
        const [x0, y0] = polar(cx, cy, r, a);
        const [x1, y1] = polar(cx, cy, r - (major ? 9 : 5), a);
        svg.appendChild(el("line", { x1: x0, y1: y0, x2: x1, y2: y1, stroke: C.tick, "stroke-width": major ? 2 : 1 }));
        if (major) {
          const [tx, ty] = polar(cx, cy, r - 18, a);
          const t = el("text", { x: tx, y: ty + 3.5, "text-anchor": "middle", fill: C.tick, "font-size": 9, "font-family": "ui-monospace, Menlo, monospace" });
          t.textContent = String(Math.round(v));
          svg.appendChild(t);
        }
      }
      // Red lines
      if (opts.redline !== undefined) this.redline(opts.redline);
      if (opts.redlineLow !== undefined) this.redline(opts.redlineLow);
      // Pointer: tapered needle rotated about the hub
      this.pointer = el("polygon", { points: `0,-${r - 3} 2.6,0 0,8 -2.6,0`, fill: C.white, transform: `translate(${cx},${cy}) rotate(${START_DEG + 90})` });
      svg.appendChild(this.pointer);
      svg.appendChild(el("circle", { cx, cy, r: 5, fill: C.white }));
      svg.appendChild(el("circle", { cx, cy, r: 2, fill: "#2a2f36" }));
      if (opts.bug) {
        this.bugEl = el("polygon", { points: "0,0 -5,-9 5,-9", fill: C.magenta });
        svg.appendChild(this.bugEl);
      }
    }
    // Digital readout box, in the gap at the bottom of the arc: the scale ends
    // at ±0.5 r below the hub (y = cy + 21 for the end labels at r − 18), so the
    // box starts below them and stays narrower than the gap between them.
    const bw = opts.digitalOnly ? 92 : 58;
    const bh = opts.digitalOnly ? 22 : 20;
    // A digital-only gauge centres its box on the hub; a dial puts it in the arc gap
    const by = opts.digitalOnly ? cy - bh / 2 : cy + 29;
    this.valueBox = el("rect", { x: cx - bw / 2, y: by, width: bw, height: bh, fill: "#000", stroke: C.white, "stroke-width": 1.2, rx: 1 });
    svg.appendChild(this.valueBox);
    this.valueText = el("text", { x: cx + bw / 2 - 4, y: by + bh - 5, "text-anchor": "end", fill: C.white, "font-size": opts.digitalOnly ? 17 : 15, "font-family": "ui-monospace, Menlo, monospace", "font-weight": 600 });
    this.valueText.textContent = "---";
    svg.appendChild(this.valueText);
    // Label
    const label = el("text", { x: cx, y: 170, "text-anchor": "middle", fill: C.label, "font-size": 11, "font-family": "-apple-system, Helvetica, Arial, sans-serif", "font-weight": 700, "letter-spacing": 0.6 });
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
    this.svg.appendChild(el("line", { x1: x0, y1: y0, x2: x1, y2: y1, stroke: C.red, "stroke-width": 3.5 }));
  }

  set(value: number, bug?: number, valid = true): void {
    const d = this.opts.decimals ?? 0;
    if (!valid || !Number.isFinite(value)) {
      this.valueText.textContent = "---";
      this.valueText.setAttribute("fill", C.amber);
      this.valueBox.setAttribute("stroke", C.amber);
      this.pointer?.setAttribute("visibility", "hidden");
      this.fillArc?.setAttribute("d", "");
      return;
    }
    this.pointer?.setAttribute("visibility", "visible");
    this.valueText.textContent = value.toFixed(d);
    if (this.pointer) {
      const a = this.angle(value);
      this.pointer.setAttribute("transform", `translate(${this.cx},${this.cy}) rotate(${(a + 90).toFixed(2)})`);
      if (this.fillArc) {
        const a0 = this.angle(this.opts.min);
        this.fillArc.setAttribute("d", a - a0 > 0.5 ? arcPath(this.cx, this.cy, this.r, a0, a) : "");
      }
    }
    if (this.bugEl && bug !== undefined) {
      const a = this.angle(bug);
      const [x, y] = polar(this.cx, this.cy, this.r + 6, a);
      this.bugEl.setAttribute("transform", `translate(${x.toFixed(2)},${y.toFixed(2)}) rotate(${(a + 90).toFixed(1)})`);
    }
    // Exceedance colouring of the readout, pointer and fill arc
    let color = C.white;
    const o = this.opts;
    if (o.redline !== undefined && value > o.redline) color = C.red;
    else if (o.redlineLow !== undefined && value < o.redlineLow) color = C.red;
    else if (o.amber && value >= o.amber[0] && value <= o.amber[1]) color = C.amber;
    this.valueText.setAttribute("fill", color);
    this.valueBox.setAttribute("stroke", color);
    this.pointer?.setAttribute("fill", color);
    this.fillArc?.setAttribute("stroke", color);
  }
}
