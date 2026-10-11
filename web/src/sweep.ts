// Parametric sweeps: steady-state performance against one operating variable,
// optionally for several values of a second one, drawn as curves on a display
// unit. Points are settled copies of engine 1 computed in the worker.
import type { SimClient, SweepPoint } from "./sim";
import { activeSignals, signal, type Signal } from "./signals";
import type { AnyState } from "./types";
import { unitSystem, FT } from "./units";

type Var = "alt" | "mach" | "disa" | "level" | "n1";
interface VarDef { key: Var; label: () => string; toSi: (v: number) => number; fromSi: (v: number) => number; def: () => [number, number]; seriesDef: () => string; decimals: number }

const VARS: VarDef[] = [
  { key: "alt", label: () => (unitSystem() === "si" ? "Altitude (m)" : "Altitude (ft)"), toSi: (v) => (unitSystem() === "si" ? v : v * FT), fromSi: (v) => (unitSystem() === "si" ? v : v / FT), def: () => (unitSystem() === "si" ? [0, 12000] : [0, 40000]), seriesDef: () => (unitSystem() === "si" ? "0, 3000, 6000, 10668" : "0, 10000, 20000, 35000"), decimals: 0 },
  { key: "mach", label: () => "Mach", toSi: (v) => v, fromSi: (v) => v, def: () => [0, 0.85], seriesDef: () => "0, 0.3, 0.6, 0.8", decimals: 2 },
  { key: "disa", label: () => "ΔISA (°C)", toSi: (v) => v, fromSi: (v) => v, def: () => [-30, 40], seriesDef: () => "-20, 0, 15, 30", decimals: 0 },
  { key: "level", label: () => "Lever (%)", toSi: (v) => v / 100, fromSi: (v) => v * 100, def: () => (activeSignals().some((sg) => sg.key === "nozzle") ? [0, 130] : [0, 100]), seriesDef: () => "30, 60, 80, 100", decimals: 0 },
  { key: "n1", label: () => "N1 (%)", toSi: (v) => v, fromSi: (v) => v, def: () => [25, 100], seriesDef: () => "60, 80, 95", decimals: 0 },
];
const COLORS = ["#19d3f5", "#ff4fd8", "#39d353", "#ffb300", "#f4f7fa", "#b388ff"];

export interface SweepContext {
  client: SimClient;
  /** Current environment (SI) and main lever position, used for the variables not swept */
  current: () => { alt: number; mach: number; disa: number; humidity: number; level: number };
  /** Whether the active engine can hold N1 directly (turbofans) */
  hasN1: () => boolean;
}

/** One-click studies: fill the fields in; kinds restricts the recipe to engine kinds.
 *  from / to / sv are in SI (metres, Mach, K, lever 0..1 as %, N1 %) and are shown in the display unit. */
interface Recipe { label: string; x: Var; y: string; s?: Var; sv?: string; from?: number; to?: number; kinds?: string[]; hint?: string }
const RECIPES: Recipe[] = [
  { label: "Thrust lapse", x: "alt", y: "thrust", s: "mach", sv: "0, 0.4, 0.78", hint: "set the lever first (e.g. Climb)" },
  { label: "TSFC vs Mach", x: "mach", y: "tsfc", s: "n1", sv: "60, 80, 95", kinds: ["turbofan"] },
  { label: "Hot-day flat rating", x: "disa", y: "thrust", from: -30, to: 40, kinds: ["turbofan"], hint: "lever at TO/GA" },
  { label: "N1 vs ΔISA", x: "disa", y: "n1", from: -30, to: 40, kinds: ["turbofan"], hint: "lever at TO/GA" },
  { label: "Surge margin", x: "n1", y: "sm_hpc", s: "alt", kinds: ["turbofan"] },
  { label: "Power vs altitude", x: "alt", y: "power", kinds: ["piston"], hint: "full throttle" },
  { label: "MAP vs altitude", x: "alt", y: "map", kinds: ["piston"], hint: "full throttle" },
  { label: "Torque vs ΔISA", x: "disa", y: "torque", from: -30, to: 40, kinds: ["turboprop"], hint: "max power lever" },
  { label: "ITT vs ΔISA", x: "disa", y: "itt", from: -30, to: 40, kinds: ["turboprop"], hint: "max power lever" },
  { label: "Fuel flow vs lever", x: "level", y: "ff", s: "alt" },
  { label: "Thrust vs Mach", x: "mach", y: "thrust", s: "alt", sv: "0, 5000, 10000", kinds: ["turbojet"], hint: "throttle at MIL (1.0) or MAX A/B (1.3)" },
  { label: "TSFC vs Mach (jet)", x: "mach", y: "tsfc", s: "level", sv: "0.8, 1.0, 1.3", kinds: ["turbojet"] },
  { label: "Nozzle vs throttle", x: "level", y: "nozzle", from: 0, to: 1.3, kinds: ["turbojet"] },
  { label: "EGT vs throttle", x: "level", y: "egt", from: 0, to: 1.3, s: "alt", kinds: ["turbojet"] },
  { label: "Torque vs collective", x: "level", y: "torque", s: "alt", sv: "0, 1500, 3000", kinds: ["turboshaft"] },
  { label: "TOT vs collective", x: "level", y: "tot", s: "disa", sv: "-20, 0, 20, 35", kinds: ["turboshaft"] },
  { label: "Power vs ΔISA", x: "disa", y: "shp", from: -30, to: 40, kinds: ["turboshaft"], hint: "collective at hover (~0.55)" },
];

export function initSweep(ctx: SweepContext): { rebuild: () => void } {
  const $ = <T extends HTMLElement = HTMLElement>(id: string): T => document.getElementById(id) as T;
  const xSel = $<HTMLSelectElement>("sw-x"), ySel = $<HTMLSelectElement>("sw-y"), sSel = $<HTMLSelectElement>("sw-s");
  const from = $<HTMLInputElement>("sw-from"), to = $<HTMLInputElement>("sw-to"), steps = $<HTMLInputElement>("sw-steps"), sVals = $<HTMLInputElement>("sw-sv");
  const btnRun = $<HTMLButtonElement>("sw-run"), btnCsv = $<HTMLButtonElement>("sw-csv"), status = $("sw-status");
  const canvas = $<HTMLCanvasElement>("sw-canvas");
  const ctx2d = canvas.getContext("2d")!;
  const varOf = (k: string): VarDef => VARS.find((v) => v.key === k) ?? VARS[0];

  // The result is kept in SI; the display unit is applied when drawing or
  // exporting, so a unit switch before, during or after a run shows the same
  // physical points correctly labelled.
  let result: { x: VarDef; y: Signal; s: VarDef | null; xsSi: number[]; svSi: number[]; ysSi: number[][]; fixedSi: [VarDef, number][] } | null = null;
  const showVal = (v: VarDef, si: number): string => String(+v.fromSi(si).toFixed(v.decimals));
  const view = (): { xs: number[]; series: { label: string; ys: number[] }[]; fixed: string } | null => {
    if (!result) return null;
    const { x, y, s, xsSi, svSi, ysSi, fixedSi } = result;
    const conv = (v: number): number => (unitSystem() === "si" ? v : y.toImp(v));
    return {
      xs: xsSi.map((v) => x.fromSi(v)),
      series: svSi.map((v, k) => ({ label: s ? `${s.label().replace(/ \(.*\)$/, "")} ${showVal(s, v)}` : y.label, ys: ysSi[k].map(conv) })),
      fixed: fixedSi.map(([v, si]) => `${v.label()} ${showVal(v, si)}`).join(" · "),
    };
  };

  const vars = (): VarDef[] => VARS.filter((v) => v.key !== "n1" || ctx.hasN1());
  const fillVars = (): void => {
    for (const sel of [xSel, sSel]) {
      const keep = sel.value;
      sel.innerHTML = sel === sSel ? `<option value="">none</option>` : "";
      for (const v of vars()) { const o = document.createElement("option"); o.value = v.key; o.textContent = v.label(); sel.appendChild(o); }
      sel.value = vars().some((v) => v.key === keep) ? keep : sel === xSel ? "alt" : "";
    }
  };
  // Help panel and recipes
  const helpBtn = $<HTMLButtonElement>("sw-help-btn"), help = $("sw-help"), recipes = $("sw-recipes");
  helpBtn.addEventListener("click", () => { help.classList.toggle("hidden"); helpBtn.classList.toggle("active", !help.classList.contains("hidden")); });
  const applyRecipe = (r: Recipe): void => {
    xSel.value = r.x; setRange();
    const xv = varOf(r.x);
    if (r.from !== undefined) from.value = showVal(xv, r.from);
    if (r.to !== undefined) to.value = showVal(xv, r.to);
    if (activeSignals().some((sg) => sg.key === r.y)) ySel.value = r.y;
    sSel.value = r.s && vars().some((v) => v.key === r.s) ? r.s : "";
    const sv = sSel.value ? varOf(sSel.value) : null;
    sVals.value = sv ? (r.sv ? r.sv.split(/[,\s]+/).map(parseFloat).filter(Number.isFinite).map((v) => showVal(sv, v)).join(", ") : sv.seriesDef()) : ""; sVals.disabled = !sSel.value;
    status.textContent = `${r.label}: ${r.hint ? r.hint + ", then " : ""}press Run sweep.`;
    help.classList.add("hidden"); helpBtn.classList.remove("active");
    btnRun.focus();
  };
  const fillRecipes = (): void => {
    recipes.innerHTML = "";
    const kind = ctx.hasN1() ? "turbofan" : activeSignals().some((sg) => sg.key === "itt") ? "turboprop" : activeSignals().some((sg) => sg.key === "nozzle") ? "turbojet" : activeSignals().some((sg) => sg.key === "nr") ? "turboshaft" : "piston";
    for (const r of RECIPES) {
      if (r.kinds && !r.kinds.includes(kind)) continue;
      if (!activeSignals().some((sg) => sg.key === r.y)) continue;
      const b = document.createElement("button"); b.className = "btn small"; b.textContent = r.label; b.title = r.hint ?? "";
      b.addEventListener("click", () => applyRecipe(r)); recipes.appendChild(b);
    }
  };
  let lastUnits = unitSystem();
  const rebuild = (): void => {
    fillVars();
    fillRecipes();
    const keep = ySel.value;
    ySel.innerHTML = "";
    for (const s of activeSignals()) { const o = document.createElement("option"); o.value = s.key; o.textContent = s.label; ySel.appendChild(o); }
    ySel.value = activeSignals().some((s) => s.key === keep) ? keep : activeSignals().some((s) => s.key === "thrust") ? "thrust" : activeSignals()[0]?.key ?? "";
    if (unitSystem() !== lastUnits) {
      // A unit switch keeps the same physical conditions: convert the retained
      // altitude range and series values instead of re-reading them as the new unit
      const conv = (v: number): number => (lastUnits === "si" ? v / FT : v * FT);
      const fmt = (v: number): string => String(Math.round(conv(v)));
      if (xSel.value === "alt") { const a = parseFloat(from.value), b = parseFloat(to.value); if (Number.isFinite(a)) from.value = fmt(a); if (Number.isFinite(b)) to.value = fmt(b); }
      if (sSel.value === "alt") sVals.value = sVals.value.split(/[,\s]+/).map(parseFloat).filter(Number.isFinite).map(fmt).join(", ");
      lastUnits = unitSystem();
      // The result is stored in SI: redraw it in the new unit rather than discard it
      draw();
    } else { setRange(); result = null; draw(); }
  };
  const setRange = (): void => { const [a, b] = varOf(xSel.value).def(); from.value = String(a); to.value = String(b); };
  xSel.addEventListener("change", setRange);
  sSel.addEventListener("change", () => { sVals.value = sSel.value ? varOf(sSel.value).seriesDef() : ""; sVals.disabled = !sSel.value; });
  sVals.disabled = true;

  const run = async (): Promise<void> => {
    const x = varOf(xSel.value), y = signal(ySel.value), s = sSel.value ? varOf(sSel.value) : null;
    if (s && s.key === x.key) { status.textContent = "Series variable must differ from the X variable."; return; }
    const n = Math.min(Math.max(parseInt(steps.value, 10) || 12, 2), 60);
    const a = parseFloat(from.value), b = parseFloat(to.value);
    if (!Number.isFinite(a) || !Number.isFinite(b)) { status.textContent = "Enter a numeric range."; return; }
    const xs = Array.from({ length: n }, (_, i) => a + ((b - a) * i) / (n - 1));
    const sv = s ? sVals.value.split(/[,\s]+/).map(parseFloat).filter(Number.isFinite).slice(0, 6) : [NaN];
    if (s && !sv.length) { status.textContent = "Enter series values, e.g. 0, 3000, 6000."; return; }
    const cur = ctx.current();
    // N1 is held only when it is swept or used as the series; otherwise the lever applies
    const base: SweepPoint = { alt: cur.alt, mach: cur.mach, disa: cur.disa, humidity: cur.humidity, level: cur.level, n1: -1 };
    const points: SweepPoint[] = [];
    for (const v of sv) for (const xv of xs) {
      const p = { ...base } as unknown as Record<string, number>;
      p[x.key] = x.toSi(xv);
      if (s) p[s.key] = s.toSi(v);
      points.push(p as unknown as SweepPoint);
    }
    btnRun.disabled = true; status.textContent = `Computing ${points.length} steady-state points…`;
    // Convert the request to SI now, under the unit system it was typed in
    const xsSi = xs.map((v) => x.toSi(v)), svSi = s ? sv.map((v) => s.toSi(v)) : [NaN];
    try {
      const states: AnyState[] = await ctx.client.sweep(points);
      const ysSi = svSi.map((_, k) => xs.map((_, i) => y.get(states[k * n + i])));
      const usesN1 = x.key === "n1" || s?.key === "n1";
      const fixedSi: [VarDef, number][] = vars().filter((v) => v.key !== x.key && v.key !== s?.key && v.key !== "n1" && (v.key !== "level" || !usesN1)).map((v) => [v, (base as unknown as Record<string, number>)[v.key]]);
      result = { x, y, s, xsSi, svSi, ysSi, fixedSi };
      status.textContent = `${points.length} points, others held at ${view()!.fixed}.`;
      draw();
    } catch (e) {
      status.textContent = `Sweep failed: ${String(e)}`;
    } finally { btnRun.disabled = false; }
  };
  btnRun.addEventListener("click", () => void run());
  btnCsv.addEventListener("click", () => {
    const v = view();
    if (!result || !v) return;
    const { x, y } = result; const { series, xs } = v;
    const unit = unitSystem() === "si" ? y.unitSi : y.unitImp;
    const head = [x.label(), ...series.map((s) => `${s.label} (${unit})`)].join(",");
    const rows = xs.map((xv, i) => [xv, ...series.map((s) => s.ys[i])].map((v) => (Number.isFinite(v) ? String(+v.toFixed(4)) : "")).join(","));
    const blob = new Blob([[head, ...rows].join("\n")], { type: "text/csv" });
    const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = `sweep_${y.key}_vs_${x.key}.csv`; a.click(); URL.revokeObjectURL(a.href);
  });

  const draw = (): void => {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (!w || !h) return;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
    const c = ctx2d;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.clearRect(0, 0, w, h);
    c.font = "10px ui-monospace, Menlo, monospace";
    const v = view();
    if (!result || !v) { c.fillStyle = "#5c6670"; c.textAlign = "center"; c.fillText("Choose the variables and press Run sweep", w / 2, h / 2); return; }
    const { x, y } = result; const { xs, series } = v;
    const padL = 52, padR = 10, padT = 18, padB = 26;
    const pw = w - padL - padR, ph = h - padT - padB;
    const all = series.flatMap((s) => s.ys).filter(Number.isFinite);
    let ymin = Math.min(...all), ymax = Math.max(...all);
    if (!Number.isFinite(ymin)) { ymin = 0; ymax = 1; }
    if (ymax - ymin < 1e-9) { ymax = ymin + 1; }
    const pad = (ymax - ymin) * 0.06; ymin -= pad; ymax += pad;
    if (ymin > 0 && ymin < (ymax - ymin) * 0.5) ymin = 0;
    const xmin = xs[0], xmax = xs[xs.length - 1];
    const X = (v: number): number => padL + ((v - xmin) / (xmax - xmin || 1)) * pw;
    const Y = (v: number): number => padT + ph - ((v - ymin) / (ymax - ymin)) * ph;
    // grid and axes
    c.strokeStyle = "#162029"; c.lineWidth = 1; c.fillStyle = "#39d353"; c.textAlign = "right";
    for (let i = 0; i <= 4; i++) {
      const v = ymin + ((ymax - ymin) * i) / 4, yy = Y(v);
      c.beginPath(); c.moveTo(padL, yy); c.lineTo(w - padR, yy); c.stroke();
      c.fillText(fmt(v), padL - 4, yy + 3);
    }
    c.textAlign = "center";
    for (let i = 0; i <= 4; i++) {
      const v = xmin + ((xmax - xmin) * i) / 4, xx = X(v);
      c.beginPath(); c.moveTo(xx, padT); c.lineTo(xx, padT + ph); c.stroke();
      c.textAlign = i === 0 ? "left" : i === 4 ? "right" : "center";
      c.fillText(fmt(v), xx, h - 12);
    }
    c.textAlign = "center"; c.fillStyle = "#8e98a4"; c.fillText(x.label(), padL + pw / 2, h - 2);
    const unit = unitSystem() === "si" ? y.unitSi : y.unitImp;
    c.textAlign = "left"; c.fillStyle = "#19d3f5"; c.fillText(`${y.label}${unit ? " (" + unit + ")" : ""}`, padL + 2, 11);
    // curves
    let lx = padL + 2 + c.measureText(`${y.label} (${unit})`).width + 16;
    series.forEach((s, k) => {
      const color = COLORS[k % COLORS.length];
      c.strokeStyle = color; c.lineWidth = 1.6; c.beginPath();
      let started = false;
      s.ys.forEach((v, i) => { if (!Number.isFinite(v)) { started = false; return; } const px = X(xs[i]), py = Y(v); if (!started) { c.moveTo(px, py); started = true; } else c.lineTo(px, py); });
      c.stroke();
      c.fillStyle = color;
      s.ys.forEach((v, i) => { if (Number.isFinite(v)) { c.beginPath(); c.arc(X(xs[i]), Y(v), 2, 0, Math.PI * 2); c.fill(); } });
      if (result!.s) { c.fillText(s.label, lx, 11); lx += c.measureText(s.label).width + 12; }
    });
  };
  new ResizeObserver(draw).observe(canvas);
  rebuild();
  return { rebuild };
}

function fmt(v: number): string {
  const a = Math.abs(v);
  return a >= 1000 ? v.toFixed(0) : a >= 100 ? v.toFixed(0) : a >= 10 ? v.toFixed(1) : v.toFixed(2);
}
