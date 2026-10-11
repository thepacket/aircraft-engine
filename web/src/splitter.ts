// Draggable vertical splitter between the display units and the right-hand
// column (data logger and strip charts). The width is a CSS variable on #app
// and is remembered in this browser.

const KEY = "aircraft-engine.right-width";
const MIN = 240;

export function initSplitter(): void {
  const app = document.getElementById("app");
  const bar = document.getElementById("splitter");
  if (!app || !bar) return;
  const apply = (w: number): void => { app.style.setProperty("--right-w", `${Math.round(w)}px`); };
  const maxWidth = (): number => Math.max(MIN, app.clientWidth * 0.6);
  try {
    const saved = parseFloat(localStorage.getItem(KEY) ?? "");
    if (Number.isFinite(saved)) apply(Math.min(Math.max(saved, MIN), maxWidth()));
  } catch { /* storage unavailable */ }

  bar.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    bar.setPointerCapture(e.pointerId);
    bar.classList.add("dragging"); document.body.classList.add("resizing");
    const right = app.getBoundingClientRect().right - parseFloat(getComputedStyle(app).paddingRight);
    let width = right - e.clientX;
    const move = (ev: PointerEvent): void => { width = Math.min(Math.max(right - ev.clientX, MIN), maxWidth()); apply(width); };
    const up = (): void => {
      bar.removeEventListener("pointermove", move); bar.removeEventListener("pointerup", up); bar.removeEventListener("pointercancel", up);
      bar.classList.remove("dragging"); document.body.classList.remove("resizing");
      try { localStorage.setItem(KEY, String(Math.round(width))); } catch { /* ignore */ }
    };
    bar.addEventListener("pointermove", move); bar.addEventListener("pointerup", up); bar.addEventListener("pointercancel", up);
  });
  bar.addEventListener("dblclick", () => { app.style.removeProperty("--right-w"); try { localStorage.removeItem(KEY); } catch { /* ignore */ } });
}
