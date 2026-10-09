import { SimClient } from "./sim";
import type { AnyState, AnySpec, EngineTarget } from "./types";
import { Dial } from "./instruments";
import { StripChart } from "./charts";
import { parseScenario, parseOnOff, ScenarioRunner } from "./scenario";
import { signal, setActiveSignals, hasSignal } from "./signals";
import { KINDS, kindDef, type KindDef, type ControlDef } from "./kinds";
import { setUnitSystem, unitSystem, fmtFuelFlow, fmtPressure, fmtTemp, FT, type UnitSystem } from "./units";
import { parseCsv, autoMap, Replay, Comparison, INPUT_CHANNELS, INPUT_UNITS, measuredSignals, type Mapping, type LogTable } from "./replay";
import { encodeShare, decodeShare } from "./share";
import { renderMarkdown } from "./markdown";
import modelMd from "../../docs/MODEL.md?raw";

const $ = <T extends HTMLElement>(id: string): T => {
  const e = document.getElementById(id);
  if (!e) throw new Error(`missing element #${id}`);
  return e as T;
};
const esc = (s: string): string => s.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[ch]!));
const LS = { scenario: "ae.scenario", spec: "ae.spec", units: "ae.units", engines: "ae.engines", charts: "ae.charts", kind: "ae.kind" };
const lsGet = (k: string): string | null => { try { return localStorage.getItem(k); } catch { return null; } };
const lsSet = (k: string, v: string): void => { try { localStorage.setItem(k, v); } catch { /* private mode */ } };
const lsDel = (k: string): void => { try { localStorage.removeItem(k); } catch { /* ignore */ } };

async function main(): Promise<void> {
  // ---- Restore settings and share link -----------------------------------
  const shared = await decodeShare(location.hash);
  let engines = shared?.engines ?? (parseInt(lsGet(LS.engines) ?? "1", 10) || 1);
  engines = engines === 2 ? 2 : 1;
  setUnitSystem((shared?.units ?? lsGet(LS.units) ?? "si") as UnitSystem);
  const initialSpec = shared?.spec ?? lsGet(LS.spec) ?? undefined;

  let client: SimClient;
  try {
    client = await SimClient.create(engines, initialSpec);
  } catch (e) {
    console.warn("stored spec rejected, using default", e);
    lsDel(LS.spec);
    client = await SimClient.create(engines);
  }
  let spec: AnySpec = client.spec;
  let kind: KindDef = kindDef(client.kind);
  setActiveSignals(kind.signals);
  (window as unknown as { __ae: unknown }).__ae = { client, parseCsv, autoMap, Replay, KINDS };

  $<HTMLSelectElement>("engine-count").value = String(engines);
  $<HTMLSelectElement>("engine-kind").value = kind.kind;
  $<HTMLSelectElement>("units").value = unitSystem();
  const setTitle = (): void => { $("engine-name").textContent = `${spec.manufacturer} ${spec.name} — ${spec.application} · core v${client.version}`; $("eicas-title").textContent = kind.eicasTitle; };
  setTitle();

  // ---- Targets ------------------------------------------------------------
  let target: EngineTarget = "all";
  let focus = 0;
  const targetRow = $("target-row");
  const refreshTargetRow = (): void => {
    targetRow.classList.toggle("hidden", engines < 2);
    if (engines < 2) { target = "all"; focus = 0; }
    document.querySelectorAll<HTMLButtonElement>(".btn.tgt").forEach((b) => b.classList.toggle("active", b.dataset.target === String(target)));
    document.querySelectorAll<HTMLButtonElement>(".btn.focus").forEach((b) => b.classList.toggle("active", b.dataset.focus === String(focus)));
    $("target-hint").textContent = engines > 1 ? `levers → ${target === "all" ? "both" : "ENG " + (Number(target) + 1)}, display → ENG ${focus + 1}` : "";
  };
  document.querySelectorAll<HTMLButtonElement>(".btn.tgt").forEach((b) => b.addEventListener("click", () => { target = b.dataset.target === "all" ? "all" : parseInt(b.dataset.target!, 10); refreshTargetRow(); }));
  document.querySelectorAll<HTMLButtonElement>(".btn.focus").forEach((b) => b.addEventListener("click", () => { focus = parseInt(b.dataset.focus!, 10); refreshTargetRow(); }));
  refreshTargetRow();
  const focusState = (): AnyState | undefined => client.latest[focus];

  // ---- Instruments --------------------------------------------------------
  let dialSets: Dial[][] = [];
  const eicas = $("eicas");
  const buildEicas = (): void => {
    eicas.innerHTML = "";
    eicas.classList.toggle("twin", engines > 1);
    if (engines > 1) { const lab = document.createElement("div"); lab.className = "eng-labels"; lab.innerHTML = "<span>ENG 1</span><span>ENG 2</span>"; eicas.appendChild(lab); }
    const primary = document.createElement("div"); primary.className = "param-row primary";
    const secondary = document.createElement("div"); secondary.className = "param-row secondary";
    dialSets = Array.from({ length: engines }, () => []);
    const mk = (row: HTMLElement, defs: typeof kind.primary): void => {
      for (const d of defs) for (let e = 0; e < engines; e++) {
        const holder = document.createElement("div"); holder.className = "dial"; row.appendChild(holder);
        dialSets[e].push(new Dial(holder, { label: d.label, unit: d.unit, min: d.min, max: d.max, redline: d.redline, redlineLow: d.redlineLow, amber: d.amber, green: d.green, decimals: d.decimals, digitalOnly: d.digitalOnly, bug: d.bug }));
      }
    };
    mk(primary, kind.primary); mk(secondary, kind.secondary);
    eicas.appendChild(primary); eicas.appendChild(secondary);
  };
  buildEicas();

  // ---- Charts -------------------------------------------------------------
  const savedCharts: Record<string, string[]> = JSON.parse(lsGet(LS.charts) ?? "{}");
  const chartSels = Array.from(document.querySelectorAll<HTMLSelectElement>(".chart-sel"));
  const chartKeys = (): string[] => { const saved = shared?.charts ?? savedCharts[kind.kind]; return (saved ?? kind.defaultCharts).map((k, i) => (hasSignal(k) ? k : kind.defaultCharts[i])); };
  let charts: StripChart[] = [];
  const buildCharts = (): void => {
    const keys = chartKeys();
    charts = [0, 1, 2, 3].map((i) => { chartSels[i].innerHTML = ""; return new StripChart($(`chart-${i}`), keys[i] ?? kind.defaultCharts[0], engines, parseFloat($<HTMLSelectElement>("chart-window").value), chartSels[i]); });
  };
  buildCharts();
  chartSels.forEach((s) => s.addEventListener("change", () => { savedCharts[kind.kind] = charts.map((c) => c.channel); lsSet(LS.charts, JSON.stringify(savedCharts)); }));
  $<HTMLSelectElement>("chart-window").addEventListener("change", (e) => { const w = parseFloat((e.target as HTMLSelectElement).value); charts.forEach((c) => (c.windowS = w)); });

  // ---- Environment --------------------------------------------------------
  const alt = $<HTMLInputElement>("alt"), mach = $<HTMLInputElement>("mach"), disa = $<HTMLInputElement>("disa"), humidity = $<HTMLInputElement>("humidity");
  let env = { alt: 0, mach: 0, disa: 0, humidity: 0.4 };
  const speedIsTas = (): boolean => kind.kind === "piston";
  const machToDisplay = (m: number): number => (speedIsTas() ? m * 340.3 * (unitSystem() === "si" ? 1 : 1.94384) : m);
  const displayToMach = (v: number): number => (speedIsTas() ? v / (unitSystem() === "si" ? 1 : 1.94384) / 340.3 : v);
  const pushEnv = (): void => client.setEnvironment(env.alt, env.mach, env.disa, env.humidity);
  const refreshEnvInputs = (): void => {
    alt.value = unitSystem() === "si" ? String(Math.round(env.alt)) : String(Math.round(env.alt / FT));
    $("speed-label").textContent = speedIsTas() ? (unitSystem() === "si" ? "TAS (m/s)" : "TAS (kt)") : "Mach";
    mach.step = speedIsTas() ? "5" : "0.02"; mach.max = speedIsTas() ? "400" : "0.9";
    mach.value = speedIsTas() ? String(Math.round(machToDisplay(env.mach))) : String(env.mach);
    disa.value = String(env.disa); humidity.value = String(Math.round(env.humidity * 100));
    document.querySelectorAll(".alt-unit").forEach((e) => (e.textContent = unitSystem() === "si" ? "m" : "ft"));
  };
  const setEnv = (a?: number, m?: number, d?: number, h?: number): void => {
    if (a !== undefined) env.alt = a; if (m !== undefined) env.mach = m; if (d !== undefined) env.disa = d; if (h !== undefined) env.humidity = h;
    refreshEnvInputs(); pushEnv();
  };
  alt.addEventListener("change", () => setEnv((parseFloat(alt.value) || 0) * (unitSystem() === "si" ? 1 : FT)));
  mach.addEventListener("change", () => setEnv(undefined, displayToMach(parseFloat(mach.value) || 0)));
  disa.addEventListener("change", () => setEnv(undefined, undefined, parseFloat(disa.value) || 0));
  humidity.addEventListener("change", () => setEnv(undefined, undefined, undefined, (parseFloat(humidity.value) || 0) / 100));
  refreshEnvInputs();

  // ---- Controls (data-driven) ---------------------------------------------
  const controlsBox = $("controls");
  interface ControlWidget { def: ControlDef; el: HTMLElement; input?: HTMLInputElement | HTMLSelectElement; status?: HTMLElement; value?: HTMLElement }
  let widgets: ControlWidget[] = [];
  const setControl = (name: string, value: number, t: EngineTarget = target): void => client.setControl(name, value, t);
  const buildControls = (): void => {
    controlsBox.innerHTML = ""; widgets = [];
    for (const def of kind.controls) {
      const span = def.width ?? 1;
      if (def.type === "toggle") {
        const btn = document.createElement("button"); btn.className = `toggle c${span}`; btn.innerHTML = `${esc(def.label)}<br><small>${esc(def.off ?? "OFF")}</small>`;
        btn.addEventListener("click", () => { const st = focusState(); setControl(def.name, st && def.get(st) >= 0.5 ? 0 : 1); });
        controlsBox.appendChild(btn); widgets.push({ def, el: btn, status: btn.querySelector<HTMLElement>("small")! });
      } else if (def.type === "momentary") {
        const btn = document.createElement("button"); btn.className = `momentary c${span}`; btn.innerHTML = `${esc(def.label)}<br><small></small>`;
        btn.addEventListener("click", () => setControl(def.name, 1));
        controlsBox.appendChild(btn); widgets.push({ def, el: btn, status: btn.querySelector<HTMLElement>("small")! });
      } else if (def.type === "select") {
        const lab = document.createElement("label"); lab.className = `sel c${span}`; lab.textContent = def.label;
        const sel = document.createElement("select");
        for (const [v, l] of def.options ?? []) { const o = document.createElement("option"); o.value = String(v); o.textContent = l; sel.appendChild(o); }
        sel.addEventListener("change", () => setControl(def.name, parseFloat(sel.value)));
        lab.appendChild(sel); controlsBox.appendChild(lab); widgets.push({ def, el: lab, input: sel });
      } else if (def.type === "slider") {
        const lab = document.createElement("label"); lab.className = `slider-label c${span}`;
        const scale = def.scale ?? 1;
        lab.innerHTML = `${esc(def.label)} <span class="val"></span>`;
        const inp = document.createElement("input"); inp.type = "range"; inp.min = String((def.min ?? 0) * scale); inp.max = String((def.max ?? 1) * scale); inp.step = String((def.step ?? 0.01) * scale);
        inp.addEventListener("input", () => { setControl(def.name, parseFloat(inp.value) / scale); lab.querySelector(".val")!.textContent = `${(parseFloat(inp.value)).toFixed(0)}%`; });
        lab.appendChild(inp);
        if (def.presets) {
          const row = document.createElement("div"); row.className = "preset-row";
          for (const [v, l] of def.presets) { const b = document.createElement("button"); b.className = "btn small"; b.textContent = l; b.addEventListener("click", () => setControl(def.name, v)); row.appendChild(b); }
          lab.appendChild(row);
        }
        controlsBox.appendChild(lab); widgets.push({ def, el: lab, input: inp, value: lab.querySelector<HTMLElement>(".val")! });
      } else {
        const lab = document.createElement("label"); lab.className = `sel c${span}`; lab.textContent = def.label;
        const inp = document.createElement("input"); inp.type = "number"; inp.step = String(def.step ?? 1);
        inp.addEventListener("change", () => setControl(def.name, parseFloat(inp.value) || 0));
        lab.appendChild(inp); controlsBox.appendChild(lab); widgets.push({ def, el: lab, input: inp });
      }
    }
    $("n1-hold-row").classList.toggle("hidden", kind.kind !== "turbofan");
  };
  buildControls();
  const refreshControls = (st: AnyState): void => {
    for (const w of widgets) {
      const v = w.def.get(st);
      if (w.def.type === "toggle") { w.el.classList.toggle("on", v >= 0.5); w.status!.textContent = w.def.status ? w.def.status(st) : v >= 0.5 ? (w.def.on ?? "ON") : (w.def.off ?? "OFF"); }
      else if (w.def.type === "momentary") { w.status!.textContent = w.def.status ? w.def.status(st) : ""; }
      else if (w.def.type === "slider") { if (document.activeElement !== w.input) { (w.input as HTMLInputElement).value = String(v * (w.def.scale ?? 1)); w.value!.textContent = `${(v * (w.def.scale ?? 1)).toFixed(0)}%`; } }
      else if (document.activeElement !== w.input) { w.input!.value = String(v); }
    }
  };
  const n1Hold = $<HTMLInputElement>("n1-hold"), n1HoldVal = $<HTMLInputElement>("n1-hold-val");
  const setN1Hold = (on: boolean, pct: number, t: EngineTarget = target): void => { n1Hold.checked = on; if (Number.isFinite(pct)) n1HoldVal.value = String(pct); if (kind.kind === "turbofan") client.setControl("n1_demand", on ? pct : -1, t); };
  n1Hold.addEventListener("change", () => setN1Hold(n1Hold.checked, parseFloat(n1HoldVal.value)));
  n1HoldVal.addEventListener("change", () => { if (n1Hold.checked) setN1Hold(true, parseFloat(n1HoldVal.value)); });
  const jumpRunning = (level: number, t: EngineTarget = "all"): void => { for (const [name, v] of kind.runningPrep) client.setControl(name, v, t); setN1Hold(false, NaN, t); client.setRunning(level, t); };
  $("btn-jump-idle").addEventListener("click", () => jumpRunning(kind.idleLevel));
  $("btn-jump-cruise").addEventListener("click", () => { setEnv(kind.cruiseEnv[0], kind.cruiseEnv[1], 0); jumpRunning(kind.cruiseLevel); });
  $("btn-cold").addEventListener("click", () => { client.reset(); setN1Hold(false, NaN, "all"); charts.forEach((c) => c.clear()); });

  // ---- Topbar -------------------------------------------------------------
  let paused = false;
  const speedSel = $<HTMLSelectElement>("sim-speed");
  const readSpeed = (): void => client.setSpeed(parseFloat(speedSel.value) || 1);
  speedSel.addEventListener("change", readSpeed); speedSel.addEventListener("input", readSpeed);
  const btnPause = $<HTMLButtonElement>("btn-pause");
  btnPause.addEventListener("click", () => { paused = !paused; client.setPaused(paused); btnPause.textContent = paused ? "Resume" : "Pause"; });
  $("btn-reset").addEventListener("click", () => { client.reset(); client.clearFaults("all"); setN1Hold(false, NaN, "all"); charts.forEach((c) => c.clear()); scenarioStop(); replayStop(); refreshFaultPanel(); });
  $<HTMLSelectElement>("units").addEventListener("change", (e) => { setUnitSystem((e.target as HTMLSelectElement).value as UnitSystem); lsSet(LS.units, unitSystem()); refreshEnvInputs(); });
  $<HTMLSelectElement>("engine-count").addEventListener("change", (e) => {
    engines = parseInt((e.target as HTMLSelectElement).value, 10) === 2 ? 2 : 1;
    lsSet(LS.engines, String(engines)); client.setEngines(engines); charts.forEach((c) => c.setEngines(engines)); buildEicas(); refreshTargetRow();
  });
  const applyKind = (k: string, specJson?: string): Promise<void> => (specJson ? client.setSpec(specJson) : client.setKind(k)).then((r) => {
    spec = r.spec; kind = kindDef(client.kind); setActiveSignals(kind.signals);
    lsSet(LS.kind, kind.kind); if (specJson) lsSet(LS.spec, client.specJson); else lsDel(LS.spec);
    $<HTMLSelectElement>("engine-kind").value = kind.kind;
    setTitle(); buildEicas(); buildControls(); buildFaultPanel(); buildCharts(); buildLessons(); specText.value = client.specJson; showSizing();
    setN1Hold(false, NaN, "all"); env = { alt: 0, mach: 0, disa: 0, humidity: 0.4 }; refreshEnvInputs(); pushEnv(); scenarioStop(); replayStop();
    logTable = null; mapping = null; mappingBox.innerHTML = ""; statsTable.innerHTML = ""; btnReplayRun.disabled = true;
  });
  $<HTMLSelectElement>("engine-kind").addEventListener("change", (e) => void applyKind((e.target as HTMLSelectElement).value));
  $("btn-share").addEventListener("click", async () => {
    const s = await encodeShare({ scenario: scenarioText.value, spec: specText.value.trim() !== client.defaultSpecJson.trim() ? specText.value : undefined, engines, units: unitSystem(), charts: charts.map((c) => c.channel) });
    const url = `${location.origin}${location.pathname}#s=${s}`;
    history.replaceState(null, "", `#s=${s}`);
    try { await navigator.clipboard.writeText(url); $("sim-time").textContent = "link copied"; } catch { prompt("Share link", url); }
  });

  // ---- Logger -------------------------------------------------------------
  const btnLog = $<HTMLButtonElement>("btn-log");
  const setLogging = (on: boolean): void => { if (on) client.loggerStart(); else client.loggerStop(); };
  btnLog.addEventListener("click", () => setLogging(!(client.latest[0]?.logger_enabled ?? false)));
  $("btn-log-clear").addEventListener("click", () => client.loggerClear());
  $<HTMLSelectElement>("log-rate").addEventListener("change", (e) => client.loggerRate(parseFloat((e.target as HTMLSelectElement).value)));
  $("btn-log-download").addEventListener("click", async () => {
    for (let e = 0; e < engines; e++) {
      const csv = await client.csv(e);
      download(`${spec.name.replace(/[^A-Za-z0-9]+/g, "_")}${engines > 1 ? "_eng" + (e + 1) : ""}_log_${new Date().toISOString().replace(/[:.]/g, "-")}.csv`, csv, "text/csv");
    }
  });

  // ---- Tabs and docs --------------------------------------------------------
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((t) => t.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((x) => x.classList.remove("active")); t.classList.add("active");
    document.querySelectorAll(".tab-body").forEach((b) => b.classList.add("hidden")); $(`tab-${t.dataset.tab}`).classList.remove("hidden");
  }));
  $("docs").innerHTML = renderMarkdown(modelMd);

  const fileInput = $<HTMLInputElement>("file-input");
  const openFile = (accept: string): Promise<{ name: string; text: string } | null> => new Promise((resolve) => {
    fileInput.accept = accept; fileInput.value = "";
    fileInput.onchange = async () => { const f = fileInput.files?.[0]; resolve(f ? { name: f.name, text: await f.text() } : null); };
    fileInput.click();
  });

  // ---- Scenario -----------------------------------------------------------
  const scenarioText = $<HTMLTextAreaElement>("scenario-text"), scenarioStatus = $("scenario-status"), presetSel = $<HTMLSelectElement>("scenario-preset");
  const loadPreset = async (file: string): Promise<void> => { const r = await fetch(`${import.meta.env.BASE_URL}scenarios/${file}`); scenarioText.value = await r.text(); lsSet(LS.scenario, scenarioText.value); };
  const buildLessons = (): void => { presetSel.innerHTML = ""; for (const [file, label] of kind.lessons) { const o = document.createElement("option"); o.value = file; o.textContent = label; presetSel.appendChild(o); } };
  buildLessons();
  presetSel.addEventListener("change", () => void loadPreset(presetSel.value));
  if (shared?.scenario) scenarioText.value = shared.scenario; else if (lsGet(LS.scenario)) scenarioText.value = lsGet(LS.scenario)!; else await loadPreset(kind.lessons[0][0]);
  scenarioText.addEventListener("input", () => lsSet(LS.scenario, scenarioText.value));
  $("btn-scenario-open").addEventListener("click", async () => { const f = await openFile(".txt"); if (f) { scenarioText.value = f.text; lsSet(LS.scenario, f.text); } });
  $("btn-scenario-save").addEventListener("click", () => download("scenario.txt", scenarioText.value, "text/plain"));

  let runner: ScenarioRunner | null = null;
  let lastNote = "";
  const btnRun = $<HTMLButtonElement>("btn-scenario-run"), btnStop = $<HTMLButtonElement>("btn-scenario-stop");
  const scenarioStop = (): void => { runner?.stop(); runner = null; btnRun.disabled = false; btnStop.disabled = true; };
  btnRun.addEventListener("click", () => {
    const { events, errors } = parseScenario(scenarioText.value);
    if (errors.length) { scenarioStatus.textContent = errors.join("\n"); scenarioStatus.classList.add("err"); return; }
    scenarioStatus.classList.remove("err"); lastNote = "";
    let scTarget: EngineTarget = "all";
    runner = new ScenarioRunner(events, {
      engine: (t) => { scTarget = t; },
      ctl: (name, v) => client.setControl(name, v, scTarget),
      starter: (on) => client.setControl("starter", on ? 1 : 0, scTarget),
      fuel: (on) => client.setControl(kind.fuelControl.name, on ? kind.fuelControl.on : kind.fuelControl.off, scTarget),
      lever: (v) => client.setControl(kind.mainLever, v, scTarget),
      n1: (p) => setN1Hold(p !== null, p ?? NaN, scTarget),
      alt: (m) => setEnv(m), mach: (v) => setEnv(undefined, v), isa: (d) => setEnv(undefined, undefined, d), humidity: (x) => setEnv(undefined, undefined, undefined, x),
      running: (lvl) => jumpRunning(lvl, scTarget),
      fault: (name, value) => { const def = kind.faults.find((f) => f.key === name || f.key === "trigger_" + name || f.key === name + "_qt_per_min" || f.key === name + "_pct"); if (!def) { lastNote = `unknown fault ${name}`; return; } applyFault(def.key, def.kind === "num" ? parseFloat(value) : parseOnOff(value) === 1, scTarget); },
      bottle: () => applyFault("fire_bottle", true, scTarget),
      log: setLogging,
      speed: (x) => { speedSel.value = String(x); readSpeed(); },
      note: (txt) => { lastNote = txt; },
      end: () => scenarioStop(),
    }, (key) => { const st = focusState(); return st ? signal(key).get(st) : NaN; });
    runner.start(client.latest[0]?.time_s ?? 0);
    btnRun.disabled = true; btnStop.disabled = false;
  });
  btnStop.addEventListener("click", scenarioStop);

  // ---- Spec editor --------------------------------------------------------
  const specText = $<HTMLTextAreaElement>("spec-text"), specStatus = $("spec-status");
  specText.value = client.specJson;
  const showSizing = (): void => {
    const z = client.sizing as Record<string, number | { wf_kg_s?: number; egt_k?: number; tt45?: number; wf?: number; overall_pressure_ratio?: number; pr?: number }>;
    specStatus.classList.remove("err");
    if (kind.kind === "turbofan") { const t = z.takeoff as { wf_kg_s: number; egt_k: number; overall_pressure_ratio: number }; specStatus.textContent = `Sized: A9 ${(z.core_nozzle_area_m2 as number).toFixed(3)} m², A19 ${(z.bypass_nozzle_area_m2 as number).toFixed(3)} m², T4 takeoff ${(z.t4_takeoff_k as number).toFixed(0)} K, takeoff FF ${fmtFuelFlow(t.wf_kg_s)}, EGT ${(t.egt_k - 273.15).toFixed(0)} °C, OPR ${t.overall_pressure_ratio.toFixed(1)}`; }
    else if (kind.kind === "turboprop") { const t = z.takeoff as { wf: number; tt45: number; pr: number }; specStatus.textContent = `Sized: PT NGV ${(z.a45_effective as number).toExponential(2)}, T4 takeoff ${(z.t4_takeoff_k as number).toFixed(0)} K, FF ${(t.wf * 7936.64).toFixed(0)} lb/h, ITT ${(t.tt45 - 273.15).toFixed(0)} °C, PR ${t.pr.toFixed(2)}`; }
    else { specStatus.textContent = `Rated ${((z.rated_power_w as number) / 745.7).toFixed(0)} hp at ${(z.rated_rpm as number).toFixed(0)} rpm`; }
  };
  showSizing();
  const applySpec = (json: string): Promise<void> => applyKind(kind.kind, json).catch((e) => { specStatus.classList.add("err"); specStatus.textContent = `Spec rejected: ${String(e)}`; });
  $("btn-spec-apply").addEventListener("click", () => void applySpec(specText.value));
  $("btn-spec-revert").addEventListener("click", () => { lsDel(LS.spec); void applyKind(kind.kind); });
  $("btn-spec-open").addEventListener("click", async () => { const f = await openFile(".json"); if (f) { specText.value = f.text; void applySpec(f.text); } });
  $("btn-spec-download").addEventListener("click", () => download(`${spec.name.replace(/[^A-Za-z0-9]+/g, "_")}.json`, specText.value, "application/json"));

  // ---- Faults -------------------------------------------------------------
  const faultsBox = $("faults");
  const faultInputs = new Map<string, HTMLInputElement>();
  const applyFault = (key: string, value: boolean | number, t: EngineTarget = target): void => client.setFaults(t, { [key]: value });
  const buildFaultPanel = (): void => {
    faultsBox.innerHTML = ""; faultInputs.clear();
    const oneshots = document.createElement("div"); oneshots.className = "oneshot";
    for (const f of kind.faults) {
      if (f.oneshot) { const b = document.createElement("button"); b.className = "btn small btn-warn"; b.textContent = f.label; b.addEventListener("click", () => applyFault(f.key, true)); oneshots.appendChild(b); continue; }
      const lab = document.createElement("label"); lab.dataset.key = f.key;
      if (f.kind === "bool") { const cb = document.createElement("input"); cb.type = "checkbox"; cb.addEventListener("change", () => applyFault(f.key, cb.checked)); lab.appendChild(cb); lab.appendChild(document.createTextNode(f.label)); faultInputs.set(f.key, cb); }
      else { lab.appendChild(document.createTextNode(f.label)); const inp = document.createElement("input"); inp.type = "number"; inp.step = String(f.step ?? 1); inp.value = String(f.def ?? 0); inp.addEventListener("change", () => applyFault(f.key, parseFloat(inp.value) || 0)); lab.appendChild(inp); faultInputs.set(f.key, inp); }
      faultsBox.appendChild(lab);
    }
    if (oneshots.children.length) faultsBox.appendChild(oneshots);
  };
  buildFaultPanel();
  const refreshFaultPanel = (): void => {
    const st = focusState(); if (!st) return;
    const f = st.faults as Record<string, boolean | number>;
    for (const def of kind.faults) {
      const inp = faultInputs.get(def.key); if (!inp) continue;
      const lab = inp.parentElement!;
      if (def.kind === "bool") { if (document.activeElement !== inp) inp.checked = Boolean(f[def.key]); lab.classList.toggle("active", Boolean(f[def.key]) && def.key !== "carb_ice_enabled"); }
      else { if (document.activeElement !== inp) inp.value = String(f[def.key]); lab.classList.toggle("active", Number(f[def.key]) !== (def.def ?? 0)); }
    }
  };
  $("btn-faults-clear").addEventListener("click", () => client.clearFaults(target));

  // ---- Replay -------------------------------------------------------------
  const replayStatus = $("replay-status"), mappingBox = $("replay-mapping"), statsTable = $<HTMLTableElement>("replay-stats");
  const btnReplayRun = $<HTMLButtonElement>("btn-replay-run"), btnReplayStop = $<HTMLButtonElement>("btn-replay-stop");
  let logTable: LogTable | null = null;
  let mapping: Mapping | null = null;
  let replay: Replay | null = null;
  let replayT0 = 0;
  let replayActive = false;
  let lastCompareT = -1;
  const comparison = new Comparison();
  const buildMapping = (): void => {
    if (!logTable || !mapping) return;
    mappingBox.innerHTML = "";
    const colSel = (ch: string): HTMLSelectElement => {
      const s = document.createElement("select");
      const none = document.createElement("option"); none.value = ""; none.textContent = "— none —"; s.appendChild(none);
      for (const c of logTable!.columns) { const o = document.createElement("option"); o.value = c; o.textContent = c; s.appendChild(o); }
      s.value = mapping!.columns[ch] ?? "";
      s.addEventListener("change", () => { mapping!.columns[ch] = s.value || undefined; });
      return s;
    };
    const unitSel = (ch: string, labels: string[]): HTMLSelectElement => {
      const s = document.createElement("select");
      labels.forEach((l, i) => { const e = document.createElement("option"); e.value = String(i); e.textContent = l; s.appendChild(e); });
      s.value = String(mapping!.units[ch] ?? 0);
      s.addEventListener("change", () => { mapping!.units[ch] = parseInt(s.value, 10); });
      return s;
    };
    const labels: Record<string, string> = { time: "time", lever: `main lever (${kind.mainLever})`, n1_cmd: "speed target (N1)", fuel_lever: `fuel (${kind.fuelControl.name})`, starter: "starter", alt: "altitude", mach: "Mach / TAS", isa: "ΔISA" };
    for (const ch of INPUT_CHANNELS) {
      const k = document.createElement("span"); k.className = "k"; k.textContent = labels[ch]; mappingBox.appendChild(k); mappingBox.appendChild(colSel(ch));
      const u = INPUT_UNITS[ch]; mappingBox.appendChild(u ? unitSel(ch, u.map((x) => x.label)) : document.createElement("span"));
    }
    for (const sg of measuredSignals()) {
      const k = document.createElement("span"); k.className = "k"; k.textContent = `${sg.label} (measured)`; mappingBox.appendChild(k); mappingBox.appendChild(colSel(sg.key));
      mappingBox.appendChild(sg.importUnits ? unitSel(sg.key, sg.importUnits.map((x) => x.label)) : document.createElement("span"));
    }
  };
  const loadLog = (text: string, name: string): void => {
    try {
      logTable = parseCsv(text); mapping = autoMap(logTable.columns);
      replayStatus.classList.remove("err"); replayStatus.textContent = `${name}: ${logTable.n} rows, ${logTable.columns.length} columns. Check the mapping, then Replay.`;
      buildMapping(); btnReplayRun.disabled = false; statsTable.innerHTML = "";
    } catch (e) { replayStatus.classList.add("err"); replayStatus.textContent = String(e); }
  };
  $("btn-replay-open").addEventListener("click", async () => { const f = await openFile(".csv,.txt"); if (f) loadLog(f.text, f.name); });
  $("btn-replay-own").addEventListener("click", async () => { const csv = await client.csv(focus); if (csv.split("\n").length < 3) { replayStatus.textContent = "The logger has no rows yet. Log a run first."; return; } loadLog(csv, "logger CSV"); });
  const replayStop = (): void => { if (!replayActive) return; replayActive = false; btnReplayRun.disabled = !logTable; btnReplayStop.disabled = true; setN1Hold(false, NaN, "all"); renderStats(); };
  const renderStats = (): void => {
    const st = comparison.stats();
    const rows = Object.entries(st);
    if (!rows.length) { statsTable.innerHTML = ""; return; }
    const conv = (key: string, v: number): number => (unitSystem() === "imp" ? signal(key).toImp(v) - signal(key).toImp(0) : v);
    const unit = (key: string): string => (unitSystem() === "imp" ? signal(key).unitImp : signal(key).unitSi);
    statsTable.innerHTML = `<thead><tr><th>channel</th><th>n</th><th>mean err</th><th>RMS err</th><th>max |err|</th></tr></thead><tbody>` +
      rows.map(([k, s]) => `<tr><td>${esc(signal(k).label)} (${unit(k)})</td><td>${s.n}</td><td>${conv(k, s.mean).toFixed(2)}</td><td>${conv(k, s.rms).toFixed(2)}</td><td>${conv(k, s.max).toFixed(2)}</td></tr>`).join("") + "</tbody>";
  };
  btnReplayRun.addEventListener("click", () => {
    if (!logTable || !mapping) return;
    try { replay = new Replay(logTable, mapping); } catch (e) { replayStatus.classList.add("err"); replayStatus.textContent = String(e); return; }
    comparison.clear(); statsTable.innerHTML = ""; charts.forEach((c) => c.clearRef());
    const first = replay.sample(0);
    client.reset();
    if (first.alt !== undefined || first.mach !== undefined) setEnv(first.alt, first.mach, first.isa);
    const core = first.measured.n2 ?? first.measured.ng ?? first.measured.rpm;
    if (core !== undefined && core > (first.measured.rpm !== undefined ? 400 : 45)) jumpRunning(first.lever ?? 0, "all");
    replayT0 = client.latest[0]?.time_s ?? 0; lastCompareT = -1; replayActive = true;
    btnReplayRun.disabled = true; btnReplayStop.disabled = false;
    replayStatus.classList.remove("err"); replayStatus.textContent = `Replaying ${replay.duration.toFixed(0)} s of log…`;
  });
  btnReplayStop.addEventListener("click", replayStop);
  const replayTick = (states: AnyState[]): void => {
    if (!replayActive || !replay) return;
    const t = states[0].time_s - replayT0;
    if (t > replay.duration) { replayStatus.textContent = `Replay finished (${replay.duration.toFixed(0)} s).`; replayStop(); return; }
    const s = replay.sample(Math.max(t, 0));
    if (s.alt !== undefined || s.mach !== undefined || s.isa !== undefined) { env = { ...env, alt: s.alt ?? env.alt, mach: s.mach ?? env.mach, disa: s.isa ?? env.disa }; pushEnv(); }
    client.setControl("starter", s.starter ? 1 : 0, "all");
    client.setControl(kind.fuelControl.name, s.fuel_lever ?? true ? kind.fuelControl.on : kind.fuelControl.off, "all");
    if (s.lever !== undefined) { if (kind.kind === "turbofan") client.setControl("n1_demand", -1, "all"); client.setControl(kind.mainLever, s.lever, "all"); }
    else if (s.n1_cmd !== undefined && kind.kind === "turbofan") client.setControl("n1_demand", s.n1_cmd, "all");
    const simT = states[0].time_s;
    for (const [key, v] of Object.entries(s.measured)) for (const c of charts) if (c.channel === key) c.pushRef(simT, v);
    if (simT - lastCompareT >= 0.25) {
      lastCompareT = simT;
      const st = states[focus];
      for (const [key, v] of Object.entries(s.measured)) comparison.add(key, signal(key).get(st), v);
      replayStatus.textContent = `Replaying… t = ${t.toFixed(0)} / ${replay.duration.toFixed(0)} s`;
    }
  };

  // ---- Rendering ----------------------------------------------------------
  const modeBadge = $("mode-badge"), warnBox = $("warnings"), readouts = $("readouts"), stationsBody = $("stations").querySelector("tbody")!, logStatus = $("log-status");
  let lastChartT = -1, lastSlow = 0;
  client.onState((states) => {
    const st = states[focus] ?? states[0];
    if (!st || st.kind !== kind.kind) return;
    runner?.tick(st.time_s);
    replayTick(states);
    renderFast(states, st);
    if (st.time_s - lastChartT >= 0.1 || st.time_s < lastChartT) { lastChartT = st.time_s; charts.forEach((c) => { c.push(st.time_s, states); c.draw(); }); }
    const now = performance.now();
    if (now - lastSlow > 200) { lastSlow = now; renderSlow(st); refreshFaultPanel(); }
  });

  function renderFast(states: AnyState[], st: AnyState): void {
    $("sim-time").textContent = `t = ${st.time_s.toFixed(1)} s`;
    modeBadge.textContent = kind.modeOf(st).toUpperCase() + (engines > 1 ? ` (E${focus + 1})` : "");
    modeBadge.className = `mode-badge ${kind.modeOf(st)}`;
    const defs = [...kind.primary, ...kind.secondary];
    states.forEach((s, i) => { const ds = dialSets[i]; if (!ds) return; defs.forEach((d, j) => ds[j]?.set(d.get(s), d.bugOf?.(s), d.valid ? d.valid(s) : true)); });
    refreshControls(st);
    const warns: string[] = [];
    states.forEach((s, i) => s.warnings.forEach((w) => warns.push(`<span class="warning${/surge margin|LOW OIL QUANTITY|NO LIGHT|bottle|CARB ICE|above cruise|ROUGH|not primed|magnetos OFF|CHIP/i.test(w) ? " amber" : ""}">${engines > 1 ? `<span class="eng-tag">E${i + 1}</span>` : ""}${esc(w)}</span>`)));
    warnBox.innerHTML = warns.join("");
    logStatus.textContent = `${st.logger_rows} rows${st.logger_enabled ? " · recording" : ""}${runner?.active ? `  ·  scenario t+${runner.elapsed.toFixed(0)} s` : ""}`;
    btnLog.textContent = st.logger_enabled ? "Stop logging" : "Start logging"; btnLog.classList.toggle("active", st.logger_enabled);
    scenarioStatus.textContent = runner?.active ? `Running — ${runner.current}${runner.pendingConditions.length ? "\nwaiting: " + runner.pendingConditions.join("; ") : ""}` : scenarioStatus.classList.contains("err") ? scenarioStatus.textContent : "Idle";
  }

  function renderSlow(st: AnyState): void {
    const kv = kind.readouts(st);
    if (lastNote) kv.unshift(["Scenario", lastNote]);
    readouts.innerHTML = kv.map(([k, v, warn]) => `<div class="readout"><div class="k">${esc(k)}</div><div class="v${warn ? " warn" : ""}">${esc(v)}</div></div>`).join("");
    stationsBody.innerHTML = kind.stations(st).map((s) => `<tr><td>${esc(s.id)}</td><td>${esc(s.label)}</td><td>${fmtTemp(s.tt_k)}</td><td>${fmtPressure(s.pt_pa)}</td><td>${s.w_kg_s.toFixed(2)} kg/s</td></tr>`).join("");
  }
}

function download(name: string, content: string, type: string): void {
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a"); a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

main().catch((e) => { document.body.innerHTML = `<pre style="color:#ff6b6b;padding:20px">Failed to start: ${String(e)}</pre>`; console.error(e); });
