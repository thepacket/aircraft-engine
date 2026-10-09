import { Sim, type EngineState, type EngineSpec } from "./sim";
import { Dial } from "./instruments";
import { StripChart } from "./charts";
import { parseScenario, ScenarioRunner } from "./scenario";

const $ = <T extends HTMLElement>(id: string): T => {
  const e = document.getElementById(id);
  if (!e) throw new Error(`missing element #${id}`);
  return e as T;
};

const FT = 0.3048;

async function main(): Promise<void> {
  const sim = await Sim.create();
  let spec = sim.spec();
  $("engine-name").textContent = `${spec.manufacturer} ${spec.name} — ${spec.application} · core v${Sim.version()}`;

  // ---- Instruments -------------------------------------------------------
  let dials = buildDials(spec);

  // ---- Charts -------------------------------------------------------------
  const chSpeeds = new StripChart($("chart-speeds"), [
    { name: "N1 %", color: "#4fc3f7", min: 0, max: 110 },
    { name: "N2 %", color: "#b388ff", min: 0, max: 110 },
  ], 90, "Spool speeds");
  const chEgt = new StripChart($("chart-egt"), [
    { name: "EGT °C", color: "#ff8a65", min: 0, max: 1000 },
  ], 90, "Exhaust gas temperature");
  const chFf = new StripChart($("chart-ff"), [
    { name: "FF kg/s", color: "#ffd54f", min: 0, max: 1.5 },
  ], 90, "Fuel flow");
  const chThrust = new StripChart($("chart-thrust"), [
    { name: "Thrust kN", color: "#81c784", min: 0, max: 130 },
  ], 90, "Net thrust");
  const charts = [chSpeeds, chEgt, chFf, chThrust];

  // ---- Controls -----------------------------------------------------------
  const btnStarter = $<HTMLButtonElement>("btn-starter");
  const btnFuel = $<HTMLButtonElement>("btn-fuel");
  const tla = $<HTMLInputElement>("tla");
  const altFt = $<HTMLInputElement>("alt-ft");
  const mach = $<HTMLInputElement>("mach");
  const disa = $<HTMLInputElement>("disa");
  let starter = false;
  let fuel = false;

  const setStarter = (on: boolean): void => {
    starter = on;
    sim.raw.set_starter(on);
    btnStarter.classList.toggle("on", on);
    btnStarter.innerHTML = `STARTER<br><small>${on ? "ENGAGED" : "OFF"}</small>`;
  };
  const setFuel = (on: boolean): void => {
    fuel = on;
    sim.raw.set_fuel_lever(on);
    btnFuel.classList.toggle("on", on);
    btnFuel.innerHTML = `FUEL LEVER<br><small>${on ? "RUN" : "CUTOFF"}</small>`;
  };
  const setTla = (pct: number): void => {
    const v = Math.min(Math.max(pct, 0), 100);
    tla.value = String(v);
    $("tla-val").textContent = `${v.toFixed(0)}%`;
    sim.raw.set_throttle(v / 100);
  };
  const pushEnv = (): void => {
    sim.raw.set_environment(parseFloat(altFt.value) * FT || 0, parseFloat(mach.value) || 0, parseFloat(disa.value) || 0);
  };
  const setEnv = (altM?: number, m?: number, dIsa?: number): void => {
    if (altM !== undefined) altFt.value = String(Math.round(altM / FT));
    if (m !== undefined) mach.value = String(m);
    if (dIsa !== undefined) disa.value = String(dIsa);
    pushEnv();
  };

  btnStarter.addEventListener("click", () => setStarter(!starter));
  btnFuel.addEventListener("click", () => setFuel(!fuel));
  tla.addEventListener("input", () => setTla(parseFloat(tla.value)));
  document.querySelectorAll<HTMLButtonElement>("[data-tla]").forEach((b) => b.addEventListener("click", () => setTla(parseFloat(b.dataset.tla!))));
  for (const el of [altFt, mach, disa]) el.addEventListener("change", pushEnv);
  $("btn-jump-idle").addEventListener("click", () => { setFuel(true); setStarter(false); setTla(0); sim.raw.set_running(0); });
  $("btn-jump-cruise").addEventListener("click", () => { setEnv(10668, 0.78, 0); setFuel(true); setStarter(false); setTla(62); sim.raw.set_running(0.62); });

  // ---- Sim loop -----------------------------------------------------------
  let paused = false;
  let speed = 1;
  const speedSel = $<HTMLSelectElement>("sim-speed");
  const readSpeed = (): void => { speed = parseFloat(speedSel.value) || 1; };
  speedSel.addEventListener("change", readSpeed);
  speedSel.addEventListener("input", readSpeed);
  const btnPause = $<HTMLButtonElement>("btn-pause");
  btnPause.addEventListener("click", () => { paused = !paused; btnPause.textContent = paused ? "Resume" : "Pause"; });
  $("btn-reset").addEventListener("click", () => {
    sim.raw.reset();
    setStarter(false); setFuel(false); setTla(0);
    charts.forEach((c) => c.clear());
    scenarioStop();
  });

  // ---- Logger -------------------------------------------------------------
  const btnLog = $<HTMLButtonElement>("btn-log");
  let logging = false;
  const setLogging = (on: boolean): void => {
    logging = on;
    if (on) sim.raw.logger_start(); else sim.raw.logger_stop();
    btnLog.textContent = on ? "Stop logging" : "Start logging";
    btnLog.classList.toggle("active", on);
  };
  btnLog.addEventListener("click", () => setLogging(!logging));
  $("btn-log-clear").addEventListener("click", () => sim.raw.logger_clear());
  $<HTMLSelectElement>("log-rate").addEventListener("change", (e) => sim.raw.logger_set_rate(parseFloat((e.target as HTMLSelectElement).value)));
  $("btn-log-download").addEventListener("click", () => {
    const csv = sim.raw.logger_csv();
    download(`${spec.name.replace(/[^A-Za-z0-9]+/g, "_")}_log_${new Date().toISOString().replace(/[:.]/g, "-")}.csv`, csv, "text/csv");
  });

  // ---- Tabs ---------------------------------------------------------------
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((t) => t.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((x) => x.classList.remove("active"));
    t.classList.add("active");
    document.querySelectorAll(".tab-body").forEach((b) => b.classList.add("hidden"));
    $(`tab-${t.dataset.tab}`).classList.remove("hidden");
  }));

  // ---- Scenario -----------------------------------------------------------
  const scenarioText = $<HTMLTextAreaElement>("scenario-text");
  const scenarioStatus = $("scenario-status");
  const presetSel = $<HTMLSelectElement>("scenario-preset");
  const presets = [
    ["01-ground-start.txt", "01 Ground start"],
    ["02-takeoff-accel.txt", "02 Idle → takeoff acceleration"],
    ["03-climb-to-cruise.txt", "03 Climb to cruise"],
    ["04-hot-day-takeoff.txt", "04 Hot day takeoff (ISA+30)"],
    ["05-shutdown.txt", "05 Shutdown"],
    ["06-inflight-restart.txt", "06 In-flight shutdown & restart"],
  ];
  for (const [file, label] of presets) {
    const o = document.createElement("option");
    o.value = file; o.textContent = label;
    presetSel.appendChild(o);
  }
  const loadPreset = async (file: string): Promise<void> => {
    const r = await fetch(`${import.meta.env.BASE_URL}scenarios/${file}`);
    scenarioText.value = await r.text();
  };
  presetSel.addEventListener("change", () => void loadPreset(presetSel.value));
  await loadPreset(presets[0][0]);

  let runner: ScenarioRunner | null = null;
  let lastNote = "";
  const btnRun = $<HTMLButtonElement>("btn-scenario-run");
  const btnStop = $<HTMLButtonElement>("btn-scenario-stop");
  const scenarioStop = (): void => {
    runner?.stop();
    runner = null;
    btnRun.disabled = false;
    btnStop.disabled = true;
  };
  btnRun.addEventListener("click", () => {
    const { events, errors } = parseScenario(scenarioText.value);
    if (errors.length) {
      scenarioStatus.textContent = errors.join("\n");
      scenarioStatus.classList.add("err");
      return;
    }
    scenarioStatus.classList.remove("err");
    lastNote = "";
    runner = new ScenarioRunner(events, {
      starter: setStarter,
      fuel: setFuel,
      tla: (v) => setTla(v * 100),
      alt: (m) => setEnv(m),
      mach: (v) => setEnv(undefined, v),
      isa: (d) => setEnv(undefined, undefined, d),
      running: (t) => { setFuel(true); setStarter(false); setTla(t * 100); sim.raw.set_running(t); },
      log: setLogging,
      speed: (x) => { speed = x; $<HTMLSelectElement>("sim-speed").value = String(x); },
      note: (txt) => { lastNote = txt; },
      end: () => scenarioStop(),
    });
    runner.start(sim.time());
    btnRun.disabled = true;
    btnStop.disabled = false;
  });
  btnStop.addEventListener("click", scenarioStop);

  // ---- Spec editor --------------------------------------------------------
  const specText = $<HTMLTextAreaElement>("spec-text");
  const specStatus = $("spec-status");
  specText.value = sim.specJson();
  const showSizing = (): void => {
    const z = sim.sizing();
    specStatus.classList.remove("err");
    specStatus.textContent = `Sized: A9 ${z.core_nozzle_area_m2.toFixed(3)} m², A19 ${z.bypass_nozzle_area_m2.toFixed(3)} m², T4 takeoff ${z.t4_takeoff_k.toFixed(0)} K, takeoff FF ${(z.takeoff.wf_kg_s * 7936.64).toFixed(0)} lb/h, EGT ${(z.takeoff.egt_k - 273.15).toFixed(0)} °C, OPR ${z.takeoff.overall_pressure_ratio.toFixed(1)}`;
  };
  showSizing();
  const applySpec = (json: string): void => {
    try {
      sim.setSpecJson(json);
      spec = sim.spec();
      specText.value = sim.specJson();
      $("engine-name").textContent = `${spec.manufacturer} ${spec.name} — ${spec.application} · core v${Sim.version()}`;
      document.querySelectorAll(".dial").forEach((d) => (d.innerHTML = ""));
      dials = buildDials(spec);
      setStarter(false); setFuel(false); setTla(0);
      charts.forEach((c) => c.clear());
      showSizing();
    } catch (e) {
      specStatus.classList.add("err");
      specStatus.textContent = `Spec rejected: ${String(e)}`;
    }
  };
  $("btn-spec-apply").addEventListener("click", () => applySpec(specText.value));
  $("btn-spec-revert").addEventListener("click", () => applySpec(sim.defaultSpecJson()));
  $("btn-spec-download").addEventListener("click", () => download(`${spec.name.replace(/[^A-Za-z0-9]+/g, "_")}.json`, specText.value, "application/json"));

  // ---- Frame loop ---------------------------------------------------------
  let lastReal = performance.now();
  let lastChart = -1;
  const frame = (now: number): void => {
    const realDt = Math.min((now - lastReal) / 1000, 0.25);
    lastReal = now;
    if (!paused) sim.step(realDt * speed);
    const st = sim.state();
    runner?.tick(st.time_s);
    render(st);
    if (st.time_s - lastChart >= 0.1 || st.time_s < lastChart) {
      lastChart = st.time_s;
      chSpeeds.push(st.time_s, { "N1 %": st.n1_pct, "N2 %": st.n2_pct });
      chEgt.push(st.time_s, { "EGT °C": st.egt_c });
      chFf.push(st.time_s, { "FF kg/s": st.fuel_flow_kg_s });
      chThrust.push(st.time_s, { "Thrust kN": st.thrust_n / 1000 });
      charts.forEach((c) => c.draw());
    }
    requestAnimationFrame(frame);
  };

  const modeBadge = $("mode-badge");
  const warnBox = $("warnings");
  const readouts = $("readouts");
  const stationsBody = $("stations").querySelector("tbody")!;
  const logStatus = $("log-status");

  function render(st: EngineState): void {
    $("sim-time").textContent = `t = ${st.time_s.toFixed(1)} s`;
    modeBadge.textContent = st.mode.toUpperCase();
    modeBadge.className = `mode-badge ${st.mode}`;
    dials.n1.set(st.n1_pct, st.n1_command_pct);
    dials.egt.set(st.egt_c);
    dials.n2.set(st.n2_pct);
    dials.ff.set(st.fuel_flow_pph / 1000);
    dials.oilp.set(st.oil_pressure_psi);
    dials.oilt.set(st.oil_temperature_c);
    dials.oilq.set(st.oil_quantity_qt);
    dials.vib.set(Math.max(st.vib_n1, st.vib_n2));

    warnBox.innerHTML = st.warnings.map((w) => `<span class="warning">${escapeHtml(w)}</span>`).join("");

    const c = st.cycle;
    const kv: [string, string, string][] = [
      ["Net thrust", (st.thrust_n / 1000).toFixed(1), "kN"],
      ["Net thrust", st.thrust_lbf.toFixed(0), "lbf"],
      ["TSFC", st.tsfc_lb_lbf_h.toFixed(3), "lb/lbf·h"],
      ["Fuel flow", st.fuel_flow_kg_s.toFixed(3), "kg/s"],
      ["Fuel used", st.fuel_used_kg.toFixed(1), "kg"],
      ["T4 (turbine inlet)", st.t4_k.toFixed(0), "K"],
      ["EGT gas (true)", st.egt_gas_c.toFixed(0), "°C"],
      ["N1 corrected", st.n1_corrected_pct.toFixed(1), "%"],
      ["N2 corrected", st.n2_corrected_pct.toFixed(1), "%"],
      ["N1 / N2", `${st.n1_rpm.toFixed(0)} / ${st.n2_rpm.toFixed(0)}`, "rpm"],
      ["Inlet mass flow", c.w2_kg_s.toFixed(1), "kg/s"],
      ["Bypass ratio", c.bypass_ratio.toFixed(2), ""],
      ["Fan PR / OPR", `${c.fan_pressure_ratio.toFixed(3)} / ${c.overall_pressure_ratio.toFixed(1)}`, ""],
      ["HPT PR / LPT PR", `${c.hpt_pressure_ratio.toFixed(2)} / ${c.lpt_pressure_ratio.toFixed(2)}`, ""],
      ["Core / bypass jet", `${c.core_nozzle.exit_velocity_m_s.toFixed(0)} / ${c.bypass_nozzle.exit_velocity_m_s.toFixed(0)}`, "m/s"],
      ["Core nozzle", `${c.core_nozzle.npr.toFixed(2)}${c.core_nozzle.choked ? " choked" : ""}`, "NPR"],
      ["Fan + booster power", (c.fan_power_w / 1e6).toFixed(2), "MW"],
      ["HPC power", (c.hpc_power_w / 1e6).toFixed(2), "MW"],
      ["Thermal / propulsive", `${(c.thermal_efficiency * 100).toFixed(1)} / ${(c.propulsive_efficiency * 100).toFixed(1)}`, "%"],
      ["Overall efficiency", (c.overall_efficiency * 100).toFixed(1), "%"],
      ["Tt2 / Pt2", `${st.inlet.tt2_k.toFixed(1)} K / ${(st.inlet.pt2_pa / 1000).toFixed(1)} kPa`, ""],
      ["OAT / TAS", `${(st.inlet.ambient.temperature_k - 273.15).toFixed(1)} °C / ${(st.inlet.true_airspeed_m_s * 1.94384).toFixed(0)} kt`, ""],
      ["Takeoff timer", st.takeoff_timer_s.toFixed(0), "s"],
      ["Run time", st.run_time_s.toFixed(0), "s"],
    ];
    if (lastNote) kv.unshift(["Scenario", lastNote, ""]);
    readouts.innerHTML = kv.map(([k, v, u]) => `<div class="readout"><div class="k">${k}</div><div class="v">${escapeHtml(v)}<span class="u">${u}</span></div></div>`).join("");

    stationsBody.innerHTML = c.stations.map((s) => `<tr><td>${s.id}</td><td>${s.label}</td><td>${s.tt_k.toFixed(1)}</td><td>${(s.tt_k - 273.15).toFixed(0)}</td><td>${(s.pt_pa / 1000).toFixed(1)}</td><td>${s.w_kg_s.toFixed(1)}</td></tr>`).join("");

    logStatus.textContent = `${st.logger_rows} rows${st.logger_enabled ? " · recording" : ""}${runner?.active ? `  ·  scenario t+${runner.elapsed.toFixed(0)} s: ${runner.current}` : ""}`;
    scenarioStatus.textContent = runner?.active ? `Running — ${runner.current}` : (scenarioStatus.classList.contains("err") ? scenarioStatus.textContent : "Idle");
  }

  requestAnimationFrame(frame);
}

function buildDials(spec: EngineSpec) {
  const L = spec.limits;
  return {
    n1: new Dial($("dial-n1"), { label: "N1", unit: "%", min: 0, max: 110, redline: L.n1_max_pct, decimals: 1, bug: true }),
    egt: new Dial($("dial-egt"), { label: "EGT", unit: "°C", min: 0, max: 1000, redline: L.egt_takeoff_c, amber: [L.egt_max_continuous_c, L.egt_takeoff_c], decimals: 0 }),
    n2: new Dial($("dial-n2"), { label: "N2", unit: "%", min: 0, max: 110, redline: L.n2_max_pct, decimals: 1 }),
    ff: new Dial($("dial-ff"), { label: "FF", unit: "×1000 lb/h", min: 0, max: 12, decimals: 2, digitalOnly: true }),
    oilp: new Dial($("dial-oilp"), { label: "OIL PRESS", unit: "psi", min: 0, max: 100, redlineLow: L.oil_pressure_min_psi, amber: [L.oil_pressure_min_psi, L.oil_pressure_caution_psi], decimals: 0 }),
    oilt: new Dial($("dial-oilt"), { label: "OIL TEMP", unit: "°C", min: -40, max: 180, redline: L.oil_temp_max_transient_c, amber: [L.oil_temp_max_continuous_c, L.oil_temp_max_transient_c], decimals: 0 }),
    oilq: new Dial($("dial-oilq"), { label: "OIL QTY", unit: "qt", min: 0, max: spec.oil.capacity_qt, amber: [0, 4], decimals: 1 }),
    vib: new Dial($("dial-vib"), { label: "VIB", unit: "units", min: 0, max: 5, amber: [L.vib_advisory, 5], decimals: 1 }),
  };
}

function download(name: string, content: string, type: string): void {
  const blob = new Blob([content], { type });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[ch]!));
}

main().catch((e) => {
  document.body.innerHTML = `<pre style="color:#ff6b6b;padding:20px">Failed to start: ${String(e)}</pre>`;
  console.error(e);
});
