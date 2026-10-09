import { SimClient } from "./sim";
import type { EngineState, EngineSpec, EngineTarget } from "./types";
import { Dial } from "./instruments";
import { StripChart } from "./charts";
import { parseScenario, ScenarioRunner } from "./scenario";
import { SIGNALS, signal } from "./signals";
import { setUnitSystem, unitSystem, fmtThrust, fmtFuelFlow, fmtMass, fmtAlt, fmtSpeed, fmtPressure, fmtMassFlow, fmtPower, fmtTemp, FT, type UnitSystem } from "./units";
import { parseCsv, autoMap, Replay, Comparison, INPUT_CHANNELS, MEASURED_CHANNELS, type Mapping, type LogTable, type MeasuredChannel } from "./replay";
import { encodeShare, decodeShare } from "./share";
import { renderMarkdown } from "./markdown";
import modelMd from "../../docs/MODEL.md?raw";

const $ = <T extends HTMLElement>(id: string): T => {
  const e = document.getElementById(id);
  if (!e) throw new Error(`missing element #${id}`);
  return e as T;
};
const esc = (s: string): string => s.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[ch]!));

const LS = { scenario: "ae.scenario", spec: "ae.spec", units: "ae.units", engines: "ae.engines", charts: "ae.charts" };
const lsGet = (k: string): string | null => { try { return localStorage.getItem(k); } catch { return null; } };
const lsSet = (k: string, v: string): void => { try { localStorage.setItem(k, v); } catch { /* private mode */ } };

const PRESETS: [string, string][] = [
  ["01-ground-start.txt", "01 Ground start"],
  ["02-takeoff-accel.txt", "02 Idle → takeoff acceleration"],
  ["03-climb-to-cruise.txt", "03 Climb to cruise"],
  ["04-hot-day-takeoff.txt", "04 Hot day takeoff (flat rating)"],
  ["05-shutdown.txt", "05 Shutdown"],
  ["06-inflight-restart.txt", "06 In-flight shutdown & windmill restart"],
  ["07-hot-start.txt", "07 Hot start (rich schedule)"],
  ["08-hung-start.txt", "08 Hung start (starter cut-out)"],
  ["09-wet-start.txt", "09 Wet start (no ignition)"],
  ["10-surge.txt", "10 Compressor surge on slam accel"],
  ["11-flameout-relight.txt", "11 Flameout and auto-relight"],
  ["12-oil-loss.txt", "12 Oil leak to seizure"],
  ["13-fire.txt", "13 Engine fire drill"],
  ["14-bleed-and-reverse.txt", "14 Bleed effects and reverse thrust"],
  ["15-twin-engine-failure.txt", "15 Twin: engine failure after V1"],
];

const FAULT_DEFS: { key: string; label: string; kind: "bool" | "num"; def?: number; step?: number; oneshot?: boolean }[] = [
  { key: "starter_inop", label: "Starter inoperative", kind: "bool" },
  { key: "starter_weak", label: "Weak starter (low duct pressure)", kind: "bool" },
  { key: "starter_early_cutout", label: "Starter drops out at 30% (hung start)", kind: "bool" },
  { key: "ignition_fail", label: "Igniters failed (wet start)", kind: "bool" },
  { key: "start_fuel_factor", label: "Start fuel schedule × (1 = normal, 1.8 = hot start)", kind: "num", def: 1, step: 0.1 },
  { key: "fuel_pump_fail", label: "Fuel supply failure (flameout)", kind: "bool" },
  { key: "oil_leak_qt_per_min", label: "Oil leak, qt/min", kind: "num", def: 0, step: 0.5 },
  { key: "fire", label: "Engine fire", kind: "bool" },
  { key: "n1_governor_fail", label: "N1 overspeed governor lost", kind: "bool" },
  { key: "compressor_damage_pct", label: "HPC surge margin lost, % points", kind: "num", def: 0, step: 1 },
  { key: "fod", label: "Foreign object damage", kind: "bool" },
  { key: "egt_probe_fail", label: "EGT harness open", kind: "bool" },
  { key: "reverser_stuck", label: "Reverser stuck", kind: "bool" },
  { key: "trigger_surge", label: "Surge now", kind: "bool", oneshot: true },
  { key: "trigger_flameout", label: "Flameout now", kind: "bool", oneshot: true },
  { key: "fire_bottle", label: "Fire bottle", kind: "bool", oneshot: true },
];

const MEAS_TO_SIGNAL: Record<MeasuredChannel, { key: string; scale: number }> = {
  n1: { key: "n1", scale: 1 }, n2: { key: "n2", scale: 1 }, egt: { key: "egt", scale: 1 }, ff: { key: "ff", scale: 1 },
  thrust: { key: "thrust", scale: 0.001 }, oilp: { key: "oilp", scale: 1 }, oilt: { key: "oilt", scale: 1 }, vib: { key: "vib", scale: 1 }, t4: { key: "t4", scale: 1 },
};

async function main(): Promise<void> {
  // ---- Restore settings and share link -----------------------------------
  const shared = await decodeShare(location.hash);
  let engines = shared?.engines ?? (parseInt(lsGet(LS.engines) ?? "1", 10) || 1);
  engines = engines === 2 ? 2 : 1;
  setUnitSystem(((shared?.units ?? lsGet(LS.units) ?? "si") as UnitSystem));
  const initialSpec = shared?.spec ?? lsGet(LS.spec) ?? undefined;

  let client: SimClient;
  try {
    client = await SimClient.create(engines, initialSpec);
  } catch (e) {
    // A stored spec that no longer parses: fall back to the default
    console.warn("stored spec rejected, using default", e);
    try { localStorage.removeItem(LS.spec); } catch { /* ignore */ }
    client = await SimClient.create(engines);
  }
  let spec = client.spec;
  // Debug hook for the browser console
  (window as unknown as { __ae: unknown }).__ae = { client, parseCsv, autoMap, Replay };

  $<HTMLSelectElement>("engine-count").value = String(engines);
  $<HTMLSelectElement>("units").value = unitSystem();
  const setTitle = (): void => { $("engine-name").textContent = `${spec.manufacturer} ${spec.name} — ${spec.application} · core v${client.version}`; };
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

  // ---- Instruments --------------------------------------------------------
  type DialSet = ReturnType<typeof buildDialSet>;
  let dialSets: DialSet[] = [];
  const eicas = $("eicas");
  const buildEicas = (): void => {
    eicas.innerHTML = "";
    eicas.classList.toggle("twin", engines > 1);
    if (engines > 1) {
      const lab = document.createElement("div"); lab.className = "eng-labels"; lab.innerHTML = "<span>ENG 1</span><span>ENG 2</span>"; eicas.appendChild(lab);
    }
    const primary = document.createElement("div"); primary.className = "param-row primary";
    const secondary = document.createElement("div"); secondary.className = "param-row secondary";
    const prim = ["n1", "egt", "n2", "ff"], sec = ["oilp", "oilt", "oilq", "vib"];
    const holders: Record<string, HTMLElement[]> = {};
    const mk = (row: HTMLElement, keys: string[]): void => {
      // Twin layout: for each parameter, engine 1 then engine 2 side by side
      for (const k of keys) for (let e = 0; e < engines; e++) {
        const d = document.createElement("div"); d.className = "dial"; row.appendChild(d);
        (holders[`${k}${e}`] ??= []).push(d);
      }
    };
    mk(primary, prim); mk(secondary, sec);
    eicas.appendChild(primary); eicas.appendChild(secondary);
    dialSets = Array.from({ length: engines }, (_, e) => buildDialSet(spec, (k) => holders[`${k}${e}`][0]));
  };
  buildEicas();

  // ---- Charts -------------------------------------------------------------
  const savedCharts = shared?.charts ?? JSON.parse(lsGet(LS.charts) ?? "null") ?? ["n1", "egt", "ff", "thrust"];
  const chartSels = Array.from(document.querySelectorAll<HTMLSelectElement>(".chart-sel"));
  const charts = [0, 1, 2, 3].map((i) => new StripChart($(`chart-${i}`), savedCharts[i] ?? "n1", engines, 90, chartSels[i]));
  chartSels.forEach((s) => s.addEventListener("change", () => lsSet(LS.charts, JSON.stringify(charts.map((c) => c.channel)))));
  $<HTMLSelectElement>("chart-window").addEventListener("change", (e) => { const w = parseFloat((e.target as HTMLSelectElement).value); charts.forEach((c) => (c.windowS = w)); });

  // ---- Controls -----------------------------------------------------------
  const btnStarter = $<HTMLButtonElement>("btn-starter"), btnFuel = $<HTMLButtonElement>("btn-fuel");
  const btnRev = $<HTMLButtonElement>("btn-reverser"), btnAi = $<HTMLButtonElement>("btn-antiice"), btnPack = $<HTMLButtonElement>("btn-pack");
  const tla = $<HTMLInputElement>("tla"), alt = $<HTMLInputElement>("alt"), mach = $<HTMLInputElement>("mach"), disa = $<HTMLInputElement>("disa");
  const n1Hold = $<HTMLInputElement>("n1-hold"), n1HoldVal = $<HTMLInputElement>("n1-hold-val");
  let env = { alt: 0, mach: 0, disa: 0 };

  const setTla = (pct: number, t: EngineTarget = target): void => {
    const v = Math.min(Math.max(pct, 0), 100);
    tla.value = String(v); $("tla-val").textContent = `${v.toFixed(0)}%`;
    client.setThrottle(v / 100, t);
  };
  const setN1Hold = (on: boolean, pct: number, t: EngineTarget = target): void => {
    n1Hold.checked = on; if (Number.isFinite(pct)) n1HoldVal.value = String(pct);
    client.setN1Demand(on ? pct : -1, t);
  };
  const pushEnv = (): void => client.setEnvironment(env.alt, env.mach, env.disa);
  const refreshEnvInputs = (): void => {
    alt.value = unitSystem() === "si" ? String(Math.round(env.alt)) : String(Math.round(env.alt / FT));
    mach.value = String(env.mach); disa.value = String(env.disa);
    document.querySelectorAll(".alt-unit").forEach((e) => (e.textContent = unitSystem() === "si" ? "m" : "ft"));
  };
  const setEnv = (a?: number, m?: number, d?: number): void => {
    if (a !== undefined) env.alt = a; if (m !== undefined) env.mach = m; if (d !== undefined) env.disa = d;
    refreshEnvInputs(); pushEnv();
  };
  alt.addEventListener("change", () => setEnv((parseFloat(alt.value) || 0) * (unitSystem() === "si" ? 1 : FT)));
  mach.addEventListener("change", () => setEnv(undefined, parseFloat(mach.value) || 0));
  disa.addEventListener("change", () => setEnv(undefined, undefined, parseFloat(disa.value) || 0));
  refreshEnvInputs();

  const focusState = (): EngineState | undefined => client.latest[focus];
  btnStarter.addEventListener("click", () => client.setStarter(!(focusState()?.controls.starter ?? false), target));
  btnFuel.addEventListener("click", () => client.setFuelLever(!(focusState()?.controls.fuel_lever ?? false), target));
  btnRev.addEventListener("click", () => client.setReverser(!(focusState()?.controls.reverser ?? false), target));
  btnAi.addEventListener("click", () => client.setAntiIce(!(focusState()?.controls.anti_ice ?? false), target));
  btnPack.addEventListener("click", () => client.setPackBleed(!(focusState()?.controls.pack_bleed ?? false), target));
  tla.addEventListener("input", () => setTla(parseFloat(tla.value)));
  document.querySelectorAll<HTMLButtonElement>("[data-tla]").forEach((b) => b.addEventListener("click", () => setTla(parseFloat(b.dataset.tla!))));
  n1Hold.addEventListener("change", () => setN1Hold(n1Hold.checked, parseFloat(n1HoldVal.value)));
  n1HoldVal.addEventListener("change", () => { if (n1Hold.checked) setN1Hold(true, parseFloat(n1HoldVal.value)); });
  $("btn-jump-idle").addEventListener("click", () => { setN1Hold(false, NaN, "all"); setTla(0, "all"); client.setRunning(0, "all"); });
  $("btn-jump-cruise").addEventListener("click", () => { setEnv(10668, 0.78, 0); setN1Hold(false, NaN, "all"); setTla(62, "all"); client.setRunning(0.62, "all"); });
  $("btn-cold").addEventListener("click", () => { client.reset(); setN1Hold(false, NaN, "all"); setTla(0, "all"); charts.forEach((c) => c.clear()); });

  // ---- Topbar -------------------------------------------------------------
  let paused = false;
  const speedSel = $<HTMLSelectElement>("sim-speed");
  const readSpeed = (): void => client.setSpeed(parseFloat(speedSel.value) || 1);
  speedSel.addEventListener("change", readSpeed); speedSel.addEventListener("input", readSpeed);
  const btnPause = $<HTMLButtonElement>("btn-pause");
  btnPause.addEventListener("click", () => { paused = !paused; client.setPaused(paused); btnPause.textContent = paused ? "Resume" : "Pause"; });
  $("btn-reset").addEventListener("click", () => { client.reset(); client.clearFaults("all"); setN1Hold(false, NaN, "all"); setTla(0, "all"); charts.forEach((c) => c.clear()); scenarioStop(); replayStop(); refreshFaultPanel(); });
  $<HTMLSelectElement>("units").addEventListener("change", (e) => { setUnitSystem((e.target as HTMLSelectElement).value as UnitSystem); lsSet(LS.units, unitSystem()); refreshEnvInputs(); });
  $<HTMLSelectElement>("engine-count").addEventListener("change", (e) => {
    engines = parseInt((e.target as HTMLSelectElement).value, 10) === 2 ? 2 : 1;
    lsSet(LS.engines, String(engines));
    client.setEngines(engines);
    charts.forEach((c) => c.setEngines(engines));
    buildEicas(); refreshTargetRow();
  });
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

  // ---- Tabs ---------------------------------------------------------------
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((t) => t.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((x) => x.classList.remove("active")); t.classList.add("active");
    document.querySelectorAll(".tab-body").forEach((b) => b.classList.add("hidden")); $(`tab-${t.dataset.tab}`).classList.remove("hidden");
  }));
  $("docs").innerHTML = renderMarkdown(modelMd);

  // ---- File open helper ---------------------------------------------------
  const fileInput = $<HTMLInputElement>("file-input");
  const openFile = (accept: string): Promise<{ name: string; text: string } | null> => new Promise((resolve) => {
    fileInput.accept = accept; fileInput.value = "";
    fileInput.onchange = async () => { const f = fileInput.files?.[0]; resolve(f ? { name: f.name, text: await f.text() } : null); };
    fileInput.click();
  });

  // ---- Scenario -----------------------------------------------------------
  const scenarioText = $<HTMLTextAreaElement>("scenario-text"), scenarioStatus = $("scenario-status"), presetSel = $<HTMLSelectElement>("scenario-preset");
  for (const [file, label] of PRESETS) { const o = document.createElement("option"); o.value = file; o.textContent = label; presetSel.appendChild(o); }
  const loadPreset = async (file: string): Promise<void> => { const r = await fetch(`${import.meta.env.BASE_URL}scenarios/${file}`); scenarioText.value = await r.text(); lsSet(LS.scenario, scenarioText.value); };
  presetSel.addEventListener("change", () => void loadPreset(presetSel.value));
  if (shared?.scenario) scenarioText.value = shared.scenario; else if (lsGet(LS.scenario)) scenarioText.value = lsGet(LS.scenario)!; else await loadPreset(PRESETS[0][0]);
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
      starter: (on) => client.setStarter(on, scTarget),
      fuel: (on) => client.setFuelLever(on, scTarget),
      tla: (v) => setTla(v * 100, scTarget),
      n1: (p) => setN1Hold(p !== null, p ?? NaN, scTarget),
      alt: (m) => setEnv(m), mach: (v) => setEnv(undefined, v), isa: (d) => setEnv(undefined, undefined, d),
      running: (t) => { setN1Hold(false, NaN, scTarget); setTla(t * 100, scTarget); client.setRunning(t, scTarget); },
      antiice: (on) => client.setAntiIce(on, scTarget), pack: (on) => client.setPackBleed(on, scTarget), reverser: (on) => client.setReverser(on, scTarget),
      fault: (name, value) => { const def = FAULT_DEFS.find((f) => f.key === name || f.key === "trigger_" + name); if (!def) { lastNote = `unknown fault ${name}`; return; } applyFault(def.key, def.kind === "num" ? parseFloat(value) : /^(on|1|true)$/i.test(value), scTarget); },
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
    const z = client.sizing;
    specStatus.classList.remove("err");
    specStatus.textContent = `Sized: A9 ${z.core_nozzle_area_m2.toFixed(3)} m², A19 ${z.bypass_nozzle_area_m2.toFixed(3)} m², T4 takeoff ${z.t4_takeoff_k.toFixed(0)} K, takeoff FF ${fmtFuelFlow(z.takeoff.wf_kg_s)}, EGT ${(z.takeoff.egt_k - 273.15).toFixed(0)} °C, OPR ${z.takeoff.overall_pressure_ratio.toFixed(1)}`;
  };
  showSizing();
  const applySpec = async (json: string): Promise<void> => {
    try {
      const r = await client.setSpec(json);
      spec = r.spec; specText.value = client.specJson; lsSet(LS.spec, client.specJson);
      setTitle(); buildEicas(); setTla(0, "all"); setN1Hold(false, NaN, "all"); charts.forEach((c) => c.clear()); showSizing();
    } catch (e) { specStatus.classList.add("err"); specStatus.textContent = `Spec rejected: ${String(e)}`; }
  };
  $("btn-spec-apply").addEventListener("click", () => void applySpec(specText.value));
  $("btn-spec-revert").addEventListener("click", () => { try { localStorage.removeItem(LS.spec); } catch { /* ignore */ } void applySpec(client.defaultSpecJson); });
  $("btn-spec-open").addEventListener("click", async () => { const f = await openFile(".json"); if (f) { specText.value = f.text; void applySpec(f.text); } });
  $("btn-spec-download").addEventListener("click", () => download(`${spec.name.replace(/[^A-Za-z0-9]+/g, "_")}.json`, specText.value, "application/json"));

  // ---- Faults -------------------------------------------------------------
  const faultsBox = $("faults");
  const faultInputs = new Map<string, HTMLInputElement>();
  const applyFault = (key: string, value: boolean | number, t: EngineTarget = target): void => {
    client.setFaults(t, { [key]: value });
  };
  const buildFaultPanel = (): void => {
    faultsBox.innerHTML = "";
    const oneshots = document.createElement("div"); oneshots.className = "oneshot";
    for (const f of FAULT_DEFS) {
      if (f.oneshot) {
        const b = document.createElement("button"); b.className = "btn small btn-warn"; b.textContent = f.label;
        b.addEventListener("click", () => applyFault(f.key, true));
        oneshots.appendChild(b); continue;
      }
      const lab = document.createElement("label"); lab.dataset.key = f.key;
      if (f.kind === "bool") {
        const cb = document.createElement("input"); cb.type = "checkbox";
        cb.addEventListener("change", () => applyFault(f.key, cb.checked));
        lab.appendChild(cb); lab.appendChild(document.createTextNode(f.label)); faultInputs.set(f.key, cb);
      } else {
        lab.appendChild(document.createTextNode(f.label));
        const inp = document.createElement("input"); inp.type = "number"; inp.step = String(f.step ?? 1); inp.value = String(f.def ?? 0);
        inp.addEventListener("change", () => applyFault(f.key, parseFloat(inp.value) || 0));
        lab.appendChild(inp); faultInputs.set(f.key, inp);
      }
      faultsBox.appendChild(lab);
    }
    faultsBox.appendChild(oneshots);
  };
  buildFaultPanel();
  const refreshFaultPanel = (): void => {
    const st = focusState(); if (!st) return;
    const f = st.faults as unknown as Record<string, boolean | number>;
    for (const def of FAULT_DEFS) {
      const inp = faultInputs.get(def.key); if (!inp) continue;
      const lab = inp.parentElement!;
      if (def.kind === "bool") { if (document.activeElement !== inp) inp.checked = Boolean(f[def.key]); lab.classList.toggle("active", Boolean(f[def.key])); }
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
      s.value = (mapping!.columns as Record<string, string | undefined>)[ch] ?? "";
      s.addEventListener("change", () => { (mapping!.columns as Record<string, string | undefined>)[ch] = s.value || undefined; });
      return s;
    };
    const unitSel = (key: keyof Mapping, opts: string[]): HTMLSelectElement => {
      const s = document.createElement("select");
      for (const o of opts) { const e = document.createElement("option"); e.value = o; e.textContent = o; s.appendChild(e); }
      s.value = String(mapping![key]);
      s.addEventListener("change", () => { (mapping as unknown as Record<string, string>)[key] = s.value; });
      return s;
    };
    const units: Partial<Record<string, [keyof Mapping, string[]]>> = { time: ["timeUnit", ["s", "ms"]], tla: ["tlaScale", ["frac", "pct", "deg"]], alt: ["altUnit", ["m", "ft"]], ff: ["ffUnit", ["kg_s", "kg_h", "pph"]], thrust: ["thrustUnit", ["n", "kn", "lbf"]], egt: ["egtUnit", ["c", "k", "f"]] };
    const labels: Record<string, string> = { time: "time", tla: "thrust lever", n1_cmd: "N1 target", fuel_lever: "fuel lever", starter: "starter", alt: "altitude", mach: "Mach", isa: "ΔISA", n1: "N1 (measured)", n2: "N2 (measured)", egt: "EGT (measured)", ff: "fuel flow (measured)", thrust: "thrust (measured)", oilp: "oil press (measured)", oilt: "oil temp (measured)", vib: "vibration (measured)", t4: "T4 (measured)" };
    for (const ch of [...INPUT_CHANNELS, ...MEASURED_CHANNELS]) {
      const k = document.createElement("span"); k.className = "k"; k.textContent = labels[ch];
      mappingBox.appendChild(k); mappingBox.appendChild(colSel(ch));
      const u = units[ch];
      mappingBox.appendChild(u ? unitSel(u[0], u[1]) : document.createElement("span"));
    }
  };
  const loadLog = (text: string, name: string): void => {
    try {
      logTable = parseCsv(text); mapping = autoMap(logTable.columns);
      replayStatus.classList.remove("err");
      replayStatus.textContent = `${name}: ${logTable.n} rows, ${logTable.columns.length} columns. Check the mapping, then Replay.`;
      buildMapping(); btnReplayRun.disabled = false; statsTable.innerHTML = "";
    } catch (e) { replayStatus.classList.add("err"); replayStatus.textContent = String(e); }
  };
  $("btn-replay-open").addEventListener("click", async () => { const f = await openFile(".csv,.txt"); if (f) loadLog(f.text, f.name); });
  $("btn-replay-own").addEventListener("click", async () => { const csv = await client.csv(focus); if (csv.split("\n").length < 3) { replayStatus.textContent = "The logger has no rows yet. Log a run first."; return; } loadLog(csv, "logger CSV"); });
  const replayStop = (): void => {
    if (!replayActive) return;
    replayActive = false; btnReplayRun.disabled = !logTable; btnReplayStop.disabled = true;
    setN1Hold(false, NaN, "all");
    renderStats();
  };
  const renderStats = (): void => {
    const st = comparison.stats();
    const rows = Object.entries(st) as [MeasuredChannel, { n: number; rms: number; mean: number; max: number; meanAbs: number }][];
    if (!rows.length) { statsTable.innerHTML = ""; return; }
    const unit = (ch: MeasuredChannel): string => { const s = signal(MEAS_TO_SIGNAL[ch].key); return unitSystem() === "imp" ? s.unitImp : s.unitSi; };
    const conv = (ch: MeasuredChannel, v: number): number => { const m = MEAS_TO_SIGNAL[ch]; const s = signal(m.key); const si = v * m.scale; return unitSystem() === "imp" ? s.toImp(si) : si; };
    statsTable.innerHTML = `<thead><tr><th>channel</th><th>n</th><th>mean err</th><th>RMS err</th><th>max |err|</th></tr></thead><tbody>` +
      rows.map(([ch, s]) => `<tr><td>${ch} (${unit(ch)})</td><td>${s.n}</td><td>${conv(ch, s.mean).toFixed(2)}</td><td>${conv(ch, s.rms).toFixed(2)}</td><td>${conv(ch, s.max).toFixed(2)}</td></tr>`).join("") + "</tbody>";
  };
  btnReplayRun.addEventListener("click", () => {
    if (!logTable || !mapping) return;
    try { replay = new Replay(logTable, mapping); } catch (e) { replayStatus.classList.add("err"); replayStatus.textContent = String(e); return; }
    comparison.clear(); statsTable.innerHTML = ""; charts.forEach((c) => c.clearRef());
    const first = replay.sample(0);
    client.reset();
    if (first.alt !== undefined || first.mach !== undefined) setEnv(first.alt, first.mach, first.isa);
    // Start the model where the log starts: running if the log shows a lit engine
    if ((first.measured.n2 ?? 0) > 50) client.setRunning(first.tla ?? 0, "all");
    replayT0 = client.latest[0]?.time_s ?? 0; lastCompareT = -1; replayActive = true;
    btnReplayRun.disabled = true; btnReplayStop.disabled = false;
    replayStatus.classList.remove("err"); replayStatus.textContent = `Replaying ${replay.duration.toFixed(0)} s of log…`;
  });
  btnReplayStop.addEventListener("click", replayStop);

  const replayTick = (states: EngineState[]): void => {
    if (!replayActive || !replay) return;
    const t = states[0].time_s - replayT0;
    if (t > replay.duration) { replayStatus.textContent = `Replay finished (${replay.duration.toFixed(0)} s).`; replayStop(); return; }
    const s = replay.sample(t);
    if (s.alt !== undefined || s.mach !== undefined || s.isa !== undefined) { env = { alt: s.alt ?? env.alt, mach: s.mach ?? env.mach, disa: s.isa ?? env.disa }; pushEnv(); }
    client.setStarter(s.starter ?? false, "all");
    client.setFuelLever(s.fuel_lever ?? true, "all");
    if (s.tla !== undefined) { client.setN1Demand(-1, "all"); setTla(s.tla * 100, "all"); }
    else if (s.n1_cmd !== undefined) client.setN1Demand(s.n1_cmd, "all");
    const simT = states[0].time_s;
    for (const [ch, v] of Object.entries(s.measured) as [MeasuredChannel, number][]) {
      const m = MEAS_TO_SIGNAL[ch];
      for (const c of charts) if (c.channel === m.key) c.pushRef(simT, v * m.scale);
    }
    if (simT - lastCompareT >= 0.25) {
      lastCompareT = simT;
      const st = states[focus];
      for (const [ch, v] of Object.entries(s.measured) as [MeasuredChannel, number][]) {
        const m = MEAS_TO_SIGNAL[ch];
        comparison.add(ch, signal(m.key).get(st) / m.scale, v);
      }
      replayStatus.textContent = `Replaying… t = ${t.toFixed(0)} / ${replay.duration.toFixed(0)} s`;
    }
  };

  // ---- Rendering ----------------------------------------------------------
  const modeBadge = $("mode-badge"), warnBox = $("warnings"), readouts = $("readouts"), stationsBody = $("stations").querySelector("tbody")!, logStatus = $("log-status");
  let lastChartT = -1, lastSlow = 0;

  client.onState((states) => {
    const st = states[focus] ?? states[0];
    if (!st) return;
    runner?.tick(st.time_s);
    replayTick(states);
    renderFast(states, st);
    if (st.time_s - lastChartT >= 0.1 || st.time_s < lastChartT) { lastChartT = st.time_s; charts.forEach((c) => { c.push(st.time_s, states); c.draw(); }); }
    const now = performance.now();
    if (now - lastSlow > 200) { lastSlow = now; renderSlow(st); refreshFaultPanel(); }
  });

  function renderFast(states: EngineState[], st: EngineState): void {
    $("sim-time").textContent = `t = ${st.time_s.toFixed(1)} s`;
    modeBadge.textContent = st.mode.toUpperCase() + (engines > 1 ? ` (E${focus + 1})` : "");
    modeBadge.className = `mode-badge ${st.mode}`;
    states.forEach((s, i) => {
      const d = dialSets[i]; if (!d) return;
      d.n1.set(s.n1_pct, s.n1_command_pct); d.egt.set(s.egt_c, undefined, s.egt_valid); d.n2.set(s.n2_pct); d.ff.set(s.fuel_flow_pph / 1000);
      d.oilp.set(s.oil_pressure_psi); d.oilt.set(s.oil_temperature_c); d.oilq.set(s.oil_quantity_qt); d.vib.set(Math.max(s.vib_n1, s.vib_n2));
    });
    const c = st.controls;
    btnStarter.classList.toggle("on", c.starter); btnStarter.innerHTML = `STARTER<br><small>${st.starter_engaged ? "ENGAGED" : c.starter ? "ARMED" : "OFF"}</small>`;
    btnFuel.classList.toggle("on", c.fuel_lever); btnFuel.innerHTML = `FUEL LEVER<br><small>${c.fuel_lever ? "RUN" : "CUTOFF"}</small>`;
    btnRev.classList.toggle("on", c.reverser); btnRev.innerHTML = `REVERSER<br><small>${st.reverser_position > 0.99 ? "DEPLOYED" : st.reverser_position > 0.01 ? "IN TRANSIT" : "STOWED"}</small>`;
    btnAi.classList.toggle("on", c.anti_ice); btnAi.innerHTML = `ENG ANTI-ICE<br><small>${c.anti_ice ? "ON" : "OFF"}</small>`;
    btnPack.classList.toggle("on", c.pack_bleed); btnPack.innerHTML = `PACK BLEED<br><small>${c.pack_bleed ? "ON" : "OFF"}</small>`;
    if (document.activeElement !== tla) { tla.value = String(c.tla * 100); $("tla-val").textContent = `${(c.tla * 100).toFixed(0)}%`; }
    const warns: string[] = [];
    states.forEach((s, i) => s.warnings.forEach((w) => warns.push(`<span class="warning${/surge margin|LOW OIL QUANTITY|NO LIGHT|bottle/i.test(w) ? " amber" : ""}">${engines > 1 ? `<span class="eng-tag">E${i + 1}</span>` : ""}${esc(w)}</span>`)));
    warnBox.innerHTML = warns.join("");
    logStatus.textContent = `${st.logger_rows} rows${st.logger_enabled ? " · recording" : ""}${runner?.active ? `  ·  scenario t+${runner.elapsed.toFixed(0)} s` : ""}`;
    btnLog.textContent = st.logger_enabled ? "Stop logging" : "Start logging"; btnLog.classList.toggle("active", st.logger_enabled);
    scenarioStatus.textContent = runner?.active ? `Running — ${runner.current}${runner.pendingConditions.length ? "\nwaiting: " + runner.pendingConditions.join("; ") : ""}` : scenarioStatus.classList.contains("err") ? scenarioStatus.textContent : "Idle";
  }

  function renderSlow(st: EngineState): void {
    const c = st.cycle;
    const kv: [string, string, boolean?][] = [
      ["Net thrust", fmtThrust(st.thrust_n)],
      ["Reverse thrust", fmtThrust(st.thrust_reverse_n)],
      ["TSFC", `${st.tsfc_lb_lbf_h.toFixed(3)} lb/lbf·h`],
      ["Fuel flow", fmtFuelFlow(st.fuel_flow_kg_s)],
      ["Fuel: steady / demand", `${fmtFuelFlow(st.fuel_flow_steady_kg_s)} / ${fmtFuelFlow(st.fuel_command_kg_s)}`],
      ["Fuel limits (decel / accel)", `${fmtFuelFlow(st.fuel_decel_limit_kg_s)} / ${fmtFuelFlow(st.fuel_accel_limit_kg_s)}`],
      ["Wf/Ps3", `${st.wf_p3_ratio.toFixed(2)} (kg/h)/kPa · Ps3 ${fmtPressure(st.ps3_kpa * 1000)}`],
      ["Fuel used", fmtMass(st.fuel_used_kg)],
      ["N1 command / limit", `${st.n1_command_pct.toFixed(1)} / ${st.n1_limit_pct.toFixed(1)} % (${st.rating_limit}${st.controls.n1_demand_pct >= 0 ? ", N1 hold" : ""})`, st.rating_limit === "EGT"],
      ["N1 idle schedule", `${st.n1_idle_pct.toFixed(1)} %`],
      ["T4 actual / steady", `${st.t4_k.toFixed(0)} / ${st.t4_steady_k.toFixed(0)} K`, Math.abs(st.t4_k - st.t4_steady_k) > 50],
      ["EGT gas (true)", `${st.egt_gas_c.toFixed(0)} °C`],
      ["N1 / N2 corrected", `${st.n1_corrected_pct.toFixed(1)} / ${st.n2_corrected_pct.toFixed(1)} %`],
      ["N1 / N2", `${st.n1_rpm.toFixed(0)} / ${st.n2_rpm.toFixed(0)} rpm`],
      ["HPC / fan surge margin", `${st.hpc_surge_margin_pct.toFixed(1)} / ${st.fan_surge_margin_pct.toFixed(1)} %`, st.hpc_surge_margin_pct < 8],
      ["VBV / VSV", `${(st.vbv_open * 100).toFixed(0)} % open / ${st.vsv_angle_deg.toFixed(0)}°`],
      ["Customer bleed", `${(st.bleed_fraction * 100).toFixed(1)} % · ${fmtMassFlow(c.bleed_kg_s)}`],
      ["Inlet mass flow", fmtMassFlow(c.w2_kg_s)],
      ["Bypass ratio", c.bypass_ratio.toFixed(2)],
      ["Fan PR / OPR", `${c.fan_pressure_ratio.toFixed(3)} / ${c.overall_pressure_ratio.toFixed(1)}`],
      ["HPT PR / LPT PR", `${c.hpt_pressure_ratio.toFixed(2)} / ${c.lpt_pressure_ratio.toFixed(2)}`],
      ["Core / bypass jet", `${fmtSpeed(c.core_nozzle.exit_velocity_m_s)} / ${fmtSpeed(c.bypass_nozzle.exit_velocity_m_s)}`],
      ["Core nozzle", `NPR ${c.core_nozzle.npr.toFixed(2)}${c.core_nozzle.choked ? " choked" : ""}`],
      ["Fan+booster / HPC power", `${fmtPower(c.fan_power_w)} / ${fmtPower(c.hpc_power_w)}`],
      ["Thermal / propulsive / overall", `${(c.thermal_efficiency * 100).toFixed(1)} / ${(c.propulsive_efficiency * 100).toFixed(1)} / ${(c.overall_efficiency * 100).toFixed(1)} %`],
      ["Tt2 / Pt2", `${fmtTemp(st.inlet.tt2_k)} / ${fmtPressure(st.inlet.pt2_pa)}`],
      ["OAT / altitude / TAS", `${fmtTemp(st.inlet.ambient.temperature_k)} / ${fmtAlt(st.environment.altitude_m)} / ${fmtSpeed(st.inlet.true_airspeed_m_s)}`],
      ["Windmill N2 at this Mach", `${st.windmill_n2_pct.toFixed(0)} %`],
      ["Surges / flameouts", `${st.surge_count} / ${st.flameout_count}`, st.surge_count + st.flameout_count > 0],
      ["Takeoff timer / run time", `${st.takeoff_timer_s.toFixed(0)} / ${st.run_time_s.toFixed(0)} s`],
    ];
    if (lastNote) kv.unshift(["Scenario", lastNote]);
    readouts.innerHTML = kv.map(([k, v, warn]) => `<div class="readout"><div class="k">${k}</div><div class="v${warn ? " warn" : ""}">${esc(v)}</div></div>`).join("");
    stationsBody.innerHTML = c.stations.map((s) => `<tr><td>${s.id}</td><td>${s.label}</td><td>${fmtTemp(s.tt_k)}</td><td>${fmtPressure(s.pt_pa)}</td><td>${fmtMassFlow(s.w_kg_s)}</td></tr>`).join("");
  }
}

function buildDialSet(spec: EngineSpec, holder: (key: string) => HTMLElement) {
  const L = spec.limits;
  return {
    n1: new Dial(holder("n1"), { label: "N1", unit: "%", min: 0, max: 110, redline: L.n1_max_pct, decimals: 1, bug: true }),
    egt: new Dial(holder("egt"), { label: "EGT", unit: "°C", min: 0, max: 1000, redline: L.egt_takeoff_c, amber: [L.egt_max_continuous_c, L.egt_takeoff_c], decimals: 0 }),
    n2: new Dial(holder("n2"), { label: "N2", unit: "%", min: 0, max: 110, redline: L.n2_max_pct, decimals: 1 }),
    ff: new Dial(holder("ff"), { label: "FF", unit: "×1000 lb/h", min: 0, max: 12, decimals: 2, digitalOnly: true }),
    oilp: new Dial(holder("oilp"), { label: "OIL PRESS", unit: "psi", min: 0, max: 100, redlineLow: L.oil_pressure_min_psi, amber: [L.oil_pressure_min_psi, L.oil_pressure_caution_psi], decimals: 0 }),
    oilt: new Dial(holder("oilt"), { label: "OIL TEMP", unit: "°C", min: -40, max: 180, redline: L.oil_temp_max_transient_c, amber: [L.oil_temp_max_continuous_c, L.oil_temp_max_transient_c], decimals: 0 }),
    oilq: new Dial(holder("oilq"), { label: "OIL QTY", unit: "qt", min: 0, max: spec.oil.capacity_qt, amber: [0, 4], decimals: 1 }),
    vib: new Dial(holder("vib"), { label: "VIB", unit: "units", min: 0, max: 5, amber: [L.vib_advisory, 5], decimals: 1 }),
  };
}

function download(name: string, content: string, type: string): void {
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a"); a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

void SIGNALS;
main().catch((e) => { document.body.innerHTML = `<pre style="color:#ff6b6b;padding:20px">Failed to start: ${String(e)}</pre>`; console.error(e); });
