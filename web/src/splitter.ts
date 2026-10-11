// Draggable splitters. Each one sets a CSS variable on #app, remembers it in
// this browser, and restores the default on double-click.
//   #splitter   vertical, between the display units and the right column
//               (data logger and strip charts): --right-w
//   #hsplitter  horizontal, under the engine indications: --eicas-h

const MIN_RIGHT = 240;
const MIN_EICAS = 180;

interface Axis {
  key: string;
  cssVar: string;
  /** Size implied by a pointer position */
  sizeAt: (e: PointerEvent) => number;
  min: number;
  max: () => number;
  /** Class set on #app while the size is user-defined */
  className?: string;
}

export function initSplitters(): void {
  const app = document.getElementById("app");
  if (!app) return;
  const center = document.getElementById("center");
  const eicas = document.querySelector<HTMLElement>(".eicas");
  const vbar = document.getElementById("splitter");
  const hbar = document.getElementById("hsplitter");

  if (vbar) attach(app, vbar, {
    key: "aircraft-engine.right-width", cssVar: "--right-w", min: MIN_RIGHT,
    max: () => Math.max(MIN_RIGHT, app.clientWidth * 0.6),
    sizeAt: (e) => app.getBoundingClientRect().right - parseFloat(getComputedStyle(app).paddingRight) - e.clientX,
  });
  if (hbar && eicas && center) attach(app, hbar, {
    key: "aircraft-engine.eicas-height", cssVar: "--eicas-h", min: MIN_EICAS, className: "eicas-sized",
    max: () => Math.max(MIN_EICAS, center.clientHeight - 120),
    sizeAt: (e) => e.clientY - eicas.getBoundingClientRect().top,
  });
}

function attach(app: HTMLElement, bar: HTMLElement, axis: Axis): void {
  const apply = (v: number): void => { app.style.setProperty(axis.cssVar, `${Math.round(v)}px`); if (axis.className) app.classList.add(axis.className); };
  const reset = (): void => { app.style.removeProperty(axis.cssVar); if (axis.className) app.classList.remove(axis.className); };
  const clamp = (v: number): number => Math.min(Math.max(v, axis.min), axis.max());
  try {
    const saved = parseFloat(localStorage.getItem(axis.key) ?? "");
    if (Number.isFinite(saved)) apply(clamp(saved));
  } catch { /* storage unavailable */ }

  bar.addEventListener("mousedown", (e) => e.preventDefault()); // no text selection while dragging
  bar.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    bar.setPointerCapture(e.pointerId);
    bar.classList.add("dragging"); document.body.classList.add("resizing");
    let size = clamp(axis.sizeAt(e));
    const move = (ev: PointerEvent): void => { size = clamp(axis.sizeAt(ev)); apply(size); };
    const up = (): void => {
      bar.removeEventListener("pointermove", move); bar.removeEventListener("pointerup", up); bar.removeEventListener("pointercancel", up);
      bar.classList.remove("dragging"); document.body.classList.remove("resizing");
      try { localStorage.setItem(axis.key, String(Math.round(size))); } catch { /* ignore */ }
    };
    bar.addEventListener("pointermove", move); bar.addEventListener("pointerup", up); bar.addEventListener("pointercancel", up);
  });
  bar.addEventListener("dblclick", () => { reset(); try { localStorage.removeItem(axis.key); } catch { /* ignore */ } });
}
