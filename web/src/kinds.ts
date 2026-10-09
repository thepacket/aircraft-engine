// Engine-kind descriptors: everything the IDE needs to present an engine
// (instruments, controls, faults, signals, readouts, lessons) in one place.
import type { AnyState } from "./types";
import { sig, id, type Signal, U_FF, U_TEMP_C, U_TEMP_F, U_THRUST_KN, U_PCT, U_RPM, U_PSI, U_INHG, U_TORQUE, U_HP } from "./signals";
import { fmtThrust, fmtFuelFlow, fmtMass, fmtAlt, fmtSpeed, fmtPressure, fmtMassFlow, fmtPower, fmtTemp, unitSystem } from "./units";

export interface DialDef {
  label: string; unit: string; min: number; max: number;
  redline?: number; redlineLow?: number; amber?: [number, number]; green?: [number, number];
  decimals?: number; digitalOnly?: boolean; bug?: boolean;
  get: (s: AnyState) => number; bugOf?: (s: AnyState) => number; valid?: (s: AnyState) => boolean;
}
export interface ControlDef {
  type: "toggle" | "slider" | "select" | "momentary" | "number";
  name: string; label: string;
  min?: number; max?: number; step?: number; scale?: number;
  options?: [number, string][];
  on?: string; off?: string;
  get: (s: AnyState) => number;
  status?: (s: AnyState) => string;
  width?: 1 | 2 | 3;
  /** lever presets shown under a slider: [value, label] */
  presets?: [number, string][];
}
export interface FaultDef { key: string; label: string; kind: "bool" | "num"; def?: number; step?: number; oneshot?: boolean }
export interface StationRow { id: string; label: string; tt_k: number; pt_pa: number; w_kg_s: number }
export interface KindDef {
  kind: string; title: string; eicasTitle: string;
  primary: DialDef[]; secondary: DialDef[];
  controls: ControlDef[];
  faults: FaultDef[];
  signals: Signal[];
  /** control names for the scenario shorthand commands */
  mainLever: string;
  fuelControl: { name: string; on: number; off: number };
  /** controls set before "jump to running" */
  runningPrep: [string, number][];
  idleLevel: number; cruiseLevel: number; cruiseEnv: [number, number];
  modeOf: (s: AnyState) => string;
  readouts: (s: AnyState) => [string, string, boolean?][];
  stations: (s: AnyState) => StationRow[];
  lessons: [string, string][];
  defaultCharts: string[];
}

const b = (v: unknown): number => (v ? 1 : 0);
const n = (v: unknown): number => (typeof v === "number" ? v : Number(v) || 0);

// --------------------------------------------------------------------------
// Turbofan: CFM56-7B26
// --------------------------------------------------------------------------
const turbofanSignals: Signal[] = [
  sig("n1", "N1", "%", "%", id, 0, 110, 1, (s) => s.n1_pct, [/^n1_pct$/i, /^n1$/i, /^n1\.1$/i, /^n1\b/i], U_PCT),
  sig("n2", "N2", "%", "%", id, 0, 110, 1, (s) => s.n2_pct, [/^n2_pct$/i, /^n2$/i, /^n2\.1$/i, /^n2\b/i], U_PCT),
  sig("n1c", "N1 corrected", "%", "%", id, 0, 110, 1, (s) => s.n1_corrected_pct),
  sig("n1_cmd", "N1 command", "%", "%", id, 0, 110, 1, (s) => s.n1_command_pct),
  sig("egt", "EGT", "°C", "°C", id, 0, 1000, 0, (s) => (s.egt_valid ? s.egt_c : NaN), [/^egt_c$/i, /^egt$/i, /^egt\.1$/i, /^egt\b/i], U_TEMP_C),
  sig("egt_gas", "EGT gas (true)", "°C", "°C", id, 0, 1000, 0, (s) => s.egt_gas_c),
  sig("t4", "T4 turbine inlet", "K", "K", id, 200, 2000, 0, (s) => s.t4_k, [/^t4_k$/i, /^t4$/i, /^tit/i], [{ label: "K", toSi: id }, { label: "°C", toSi: (v) => v + 273.15 }]),
  sig("ff", "Fuel flow", "kg/s", "lb/h", (v) => v * 7936.64, 0, 1.5, 3, (s) => s.fuel_flow_kg_s, [/^fuel_flow_kg_s$/i, /^ff$/i, /^ff\.1$/i, /fuel.?flow/i, /^wf/i], U_FF),
  sig("ff_cmd", "Fuel command", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_command_kg_s),
  sig("ff_max", "Accel limit", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_accel_limit_kg_s),
  sig("ff_min", "Decel limit", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_decel_limit_kg_s),
  sig("wf_p3", "Wf/Ps3", "(kg/h)/kPa", "(kg/h)/kPa", id, 0, 3, 2, (s) => s.wf_p3_ratio),
  sig("ps3", "Ps3 burner pressure", "kPa", "psi", (v) => v / 6.89476, 0, 3500, 0, (s) => s.ps3_kpa),
  sig("thrust", "Net thrust", "kN", "lbf", (v) => v * 224.809, -40, 130, 1, (s) => s.thrust_n / 1000, [/^thrust_n$/i, /^thrust/i, /^fn$/i], U_THRUST_KN),
  sig("tsfc", "TSFC", "lb/lbf·h", "lb/lbf·h", id, 0, 1.5, 3, (s) => s.tsfc_lb_lbf_h),
  sig("oilp", "Oil pressure", "psi", "psi", id, 0, 100, 0, (s) => s.oil_pressure_psi, [/oil.?press/i, /^oilp/i], U_PSI),
  sig("oilt", "Oil temperature", "°C", "°C", id, -40, 180, 0, (s) => s.oil_temperature_c, [/oil.?temp/i, /^oilt/i], U_TEMP_C),
  sig("oilq", "Oil quantity", "qt", "qt", id, 0, 22, 1, (s) => s.oil_quantity_qt),
  sig("vib", "Vibration", "units", "units", id, 0, 5, 1, (s) => Math.max(s.vib_n1, s.vib_n2), [/^vib/i, /vibration/i]),
  sig("sm_hpc", "HPC surge margin", "%", "%", id, -10, 30, 1, (s) => s.hpc_surge_margin_pct),
  sig("sm_fan", "Fan surge margin", "%", "%", id, -10, 30, 1, (s) => s.fan_surge_margin_pct),
  sig("vbv", "VBV position", "open", "open", id, 0, 1, 2, (s) => s.vbv_open),
  sig("vsv", "VSV angle", "°", "°", id, 0, 45, 0, (s) => s.vsv_angle_deg),
  sig("bleed", "Customer bleed", "kg/s", "lb/s", (v) => v * 2.20462, 0, 6, 2, (s) => s.cycle.bleed_kg_s),
  sig("rev", "Reverser position", "", "", id, 0, 1, 2, (s) => s.reverser_position),
  sig("tla", "Thrust lever", "", "", id, 0, 1, 2, (s) => n(s.controls.tla)),
  sig("alt", "Altitude", "m", "ft", (v) => v / 0.3048, 0, 13000, 0, (s) => s.environment.altitude_m),
  sig("mach", "Mach", "", "", id, 0, 1, 2, (s) => s.environment.mach),
  sig("opr", "Overall PR", "", "", id, 0, 40, 1, (s) => s.cycle.overall_pressure_ratio),
  sig("w2", "Inlet mass flow", "kg/s", "lb/s", (v) => v * 2.20462, 0, 400, 1, (s) => s.cycle.w2_kg_s),
  sig("bpr", "Bypass ratio", "", "", id, 0, 10, 2, (s) => s.cycle.bypass_ratio),
  sig("eff_th", "Thermal efficiency", "%", "%", id, 0, 60, 1, (s) => s.cycle.thermal_efficiency * 100),
  sig("eff_pr", "Propulsive efficiency", "%", "%", id, 0, 100, 1, (s) => s.cycle.propulsive_efficiency * 100),
];

const turbofan: KindDef = {
  kind: "turbofan",
  title: "Turbofan",
  eicasTitle: "Engine indications (737NG EICAS style)",
  primary: [
    { label: "N1", unit: "%", min: 0, max: 110, redline: 104, decimals: 1, bug: true, get: (s) => s.n1_pct, bugOf: (s) => s.n1_command_pct },
    { label: "EGT", unit: "°C", min: 0, max: 1000, redline: 950, amber: [925, 950], decimals: 0, get: (s) => s.egt_c, valid: (s) => s.egt_valid },
    { label: "N2", unit: "%", min: 0, max: 110, redline: 105, decimals: 1, get: (s) => s.n2_pct },
    { label: "FF", unit: "×1000 lb/h", min: 0, max: 12, decimals: 2, digitalOnly: true, get: (s) => s.fuel_flow_pph / 1000 },
  ],
  secondary: [
    { label: "OIL PRESS", unit: "psi", min: 0, max: 100, redlineLow: 13, amber: [13, 20], decimals: 0, get: (s) => s.oil_pressure_psi },
    { label: "OIL TEMP", unit: "°C", min: -40, max: 180, redline: 160, amber: [155, 160], decimals: 0, get: (s) => s.oil_temperature_c },
    { label: "OIL QTY", unit: "qt", min: 0, max: 22, amber: [0, 4], decimals: 1, get: (s) => s.oil_quantity_qt },
    { label: "VIB", unit: "units", min: 0, max: 5, amber: [4, 5], decimals: 1, get: (s) => Math.max(s.vib_n1, s.vib_n2) },
  ],
  controls: [
    { type: "toggle", name: "starter", label: "STARTER", on: "ENGAGED", off: "OFF", get: (s) => b(s.controls.starter), status: (s) => (s.starter_engaged ? "ENGAGED" : s.controls.starter ? "ARMED" : "OFF"), width: 1 },
    { type: "toggle", name: "fuel_lever", label: "FUEL LEVER", on: "RUN", off: "CUTOFF", get: (s) => b(s.controls.fuel_lever), width: 1 },
    { type: "slider", name: "tla", label: "Thrust lever", min: 0, max: 1, step: 0.005, scale: 100, get: (s) => n(s.controls.tla), presets: [[0, "Idle"], [0.55, "Approach"], [0.8, "Climb"], [1, "TO/GA"]], width: 3 },
    { type: "toggle", name: "reverser", label: "REVERSER", on: "DEPLOY", off: "STOWED", get: (s) => b(s.controls.reverser), status: (s) => (s.reverser_position > 0.99 ? "DEPLOYED" : s.reverser_position > 0.01 ? "IN TRANSIT" : "STOWED") },
    { type: "toggle", name: "anti_ice", label: "ENG ANTI-ICE", on: "ON", off: "OFF", get: (s) => b(s.controls.anti_ice) },
    { type: "toggle", name: "pack_bleed", label: "PACK BLEED", on: "ON", off: "OFF", get: (s) => b(s.controls.pack_bleed) },
  ],
  faults: [
    { key: "starter_inop", label: "Starter inoperative", kind: "bool" },
    { key: "starter_weak", label: "Weak starter (low duct pressure)", kind: "bool" },
    { key: "starter_early_cutout", label: "Starter drops out at 30% (hung start)", kind: "bool" },
    { key: "ignition_fail", label: "Igniters failed (wet start)", kind: "bool" },
    { key: "start_fuel_factor", label: "Start fuel schedule × (1.8 = hot start)", kind: "num", def: 1, step: 0.1 },
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
  ],
  signals: turbofanSignals,
  mainLever: "tla",
  fuelControl: { name: "fuel_lever", on: 1, off: 0 },
  runningPrep: [["fuel_lever", 1], ["starter", 0]],
  idleLevel: 0, cruiseLevel: 0.62, cruiseEnv: [10668, 0.78],
  modeOf: (s) => s.mode,
  readouts: (s) => {
    const c = s.cycle;
    return [
      ["Net thrust", fmtThrust(s.thrust_n)],
      ["Reverse thrust", fmtThrust(s.thrust_reverse_n)],
      ["TSFC", `${s.tsfc_lb_lbf_h.toFixed(3)} lb/lbf·h`],
      ["Fuel flow", fmtFuelFlow(s.fuel_flow_kg_s)],
      ["Fuel: steady / demand", `${fmtFuelFlow(s.fuel_flow_steady_kg_s)} / ${fmtFuelFlow(s.fuel_command_kg_s)}`],
      ["Fuel limits (decel / accel)", `${fmtFuelFlow(s.fuel_decel_limit_kg_s)} / ${fmtFuelFlow(s.fuel_accel_limit_kg_s)}`],
      ["Wf/Ps3", `${s.wf_p3_ratio.toFixed(2)} (kg/h)/kPa · Ps3 ${fmtPressure(s.ps3_kpa * 1000)}`],
      ["Fuel used", fmtMass(s.fuel_used_kg)],
      ["N1 command / limit", `${s.n1_command_pct.toFixed(1)} / ${s.n1_limit_pct.toFixed(1)} % (${s.rating_limit}${n(s.controls.n1_demand_pct) >= 0 ? ", N1 hold" : ""})`, s.rating_limit === "EGT"],
      ["N1 idle schedule", `${s.n1_idle_pct.toFixed(1)} %`],
      ["T4 actual / steady", `${s.t4_k.toFixed(0)} / ${s.t4_steady_k.toFixed(0)} K`, Math.abs(s.t4_k - s.t4_steady_k) > 50],
      ["EGT gas (true)", `${s.egt_gas_c.toFixed(0)} °C`],
      ["N1 / N2 corrected", `${s.n1_corrected_pct.toFixed(1)} / ${s.n2_corrected_pct.toFixed(1)} %`],
      ["N1 / N2", `${s.n1_rpm.toFixed(0)} / ${s.n2_rpm.toFixed(0)} rpm`],
      ["HPC / fan surge margin", `${s.hpc_surge_margin_pct.toFixed(1)} / ${s.fan_surge_margin_pct.toFixed(1)} %`, s.hpc_surge_margin_pct < 8],
      ["VBV / VSV", `${(s.vbv_open * 100).toFixed(0)} % open / ${s.vsv_angle_deg.toFixed(0)}°`],
      ["Customer bleed", `${(s.bleed_fraction * 100).toFixed(1)} % · ${fmtMassFlow(c.bleed_kg_s)}`],
      ["Inlet mass flow", fmtMassFlow(c.w2_kg_s)],
      ["Bypass ratio", c.bypass_ratio.toFixed(2)],
      ["Fan PR / OPR", `${c.fan_pressure_ratio.toFixed(3)} / ${c.overall_pressure_ratio.toFixed(1)}`],
      ["HPT PR / LPT PR", `${c.hpt_pressure_ratio.toFixed(2)} / ${c.lpt_pressure_ratio.toFixed(2)}`],
      ["Core / bypass jet", `${fmtSpeed(c.core_nozzle.exit_velocity_m_s)} / ${fmtSpeed(c.bypass_nozzle.exit_velocity_m_s)}`],
      ["Core nozzle", `NPR ${c.core_nozzle.npr.toFixed(2)}${c.core_nozzle.choked ? " choked" : ""}`],
      ["Fan+booster / HPC power", `${fmtPower(c.fan_power_w)} / ${fmtPower(c.hpc_power_w)}`],
      ["Thermal / propulsive / overall", `${(c.thermal_efficiency * 100).toFixed(1)} / ${(c.propulsive_efficiency * 100).toFixed(1)} / ${(c.overall_efficiency * 100).toFixed(1)} %`],
      ["Tt2 / Pt2", `${fmtTemp(s.inlet.tt2_k)} / ${fmtPressure(s.inlet.pt2_pa)}`],
      ["OAT / altitude / TAS", `${fmtTemp(s.inlet.ambient.temperature_k)} / ${fmtAlt(s.environment.altitude_m)} / ${fmtSpeed(s.inlet.true_airspeed_m_s)}`],
      ["Windmill N2 at this Mach", `${s.windmill_n2_pct.toFixed(0)} %`],
      ["Surges / flameouts", `${s.surge_count} / ${s.flameout_count}`, s.surge_count + s.flameout_count > 0],
      ["Takeoff timer / run time", `${s.takeoff_timer_s.toFixed(0)} / ${s.run_time_s.toFixed(0)} s`],
    ];
  },
  stations: (s) => s.cycle.stations,
  lessons: [
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
  ],
  defaultCharts: ["n1", "egt", "ff", "thrust"],
};

// --------------------------------------------------------------------------
// Piston: Lycoming O-360
// --------------------------------------------------------------------------
const pistonSignals: Signal[] = [
  sig("rpm", "RPM", "rpm", "rpm", id, 0, 3000, 0, (s) => s.rpm, [/^rpm$/i, /^e1.?rpm/i, /^rpm\b/i, /^prop.?rpm/i], U_RPM),
  sig("map", "Manifold pressure", "inHg", "inHg", id, 0, 32, 1, (s) => s.map_inhg, [/^map/i, /manifold/i, /^e1.?map/i], U_INHG),
  sig("egt", "EGT", "°F", "°F", id, 0, 1700, 0, (s) => s.egt_f, [/^egt/i, /^e1.?egt/i], U_TEMP_F),
  sig("cht", "CHT", "°F", "°F", id, 0, 550, 0, (s) => s.cht_f, [/^cht/i, /^e1.?cht/i], U_TEMP_F),
  sig("ff", "Fuel flow", "kg/s", "gal/h", (v) => v / (0.72 * 3.78541 / 3600), 0, 0.02, 4, (s) => s.fuel_flow_kg_s, [/^fuel_flow_gph$/i, /^ff$/i, /fuel.?flow/i, /^e1.?ff/i, /^gph/i], U_FF),
  sig("ff_gph", "Fuel flow", "gal/h", "gal/h", id, 0, 20, 1, (s) => s.fuel_flow_gph),
  sig("oilp", "Oil pressure", "psi", "psi", id, 0, 120, 0, (s) => s.oil_pressure_psi, [/oil.?press/i, /^oilp/i, /^e1.?oilp/i], U_PSI),
  sig("oilt", "Oil temperature", "°F", "°F", id, 0, 260, 0, (s) => s.oil_temperature_f, [/oil.?temp/i, /^oilt/i, /^e1.?oilt/i], U_TEMP_F),
  sig("fuelp", "Fuel pressure", "psi", "psi", id, 0, 10, 1, (s) => s.fuel_pressure_psi, [/fuel.?press/i]),
  sig("power", "Brake power", "hp", "hp", id, 0, 200, 0, (s) => s.power_hp, [/^power/i, /^hp$/i, /^bhp/i], U_HP),
  sig("power_pct", "Power", "%", "%", id, 0, 110, 0, (s) => s.power_pct, [/pct.?pwr/i, /power.?pct/i, /%.?pwr/i], U_PCT),
  sig("fa", "Fuel/air ratio", "", "", id, 0, 0.12, 4, (s) => s.fuel_air_ratio),
  sig("thrust", "Prop thrust", "kN", "lbf", (v) => v * 224.809, 0, 4, 2, (s) => s.thrust_n / 1000),
  sig("eta_p", "Prop efficiency", "", "", id, 0, 1, 2, (s) => s.prop.efficiency),
  sig("bsfc", "BSFC", "lb/hp·h", "lb/hp·h", id, 0, 1.5, 2, (s) => s.bsfc_lb_hp_h),
  sig("carb_ice", "Carb ice", "", "", id, 0, 1, 2, (s) => s.carb_ice),
  sig("rough", "Roughness", "", "", id, 0, 1.5, 2, (s) => s.roughness),
  sig("throttle", "Throttle", "", "", id, 0, 1, 2, (s) => n(s.controls.throttle)),
  sig("mixture", "Mixture", "", "", id, 0, 1, 2, (s) => n(s.controls.mixture)),
  sig("alt", "Altitude", "m", "ft", (v) => v / 0.3048, 0, 5000, 0, (s) => s.environment.altitude_m),
  sig("tas", "True airspeed", "m/s", "kt", (v) => v * 1.94384, 0, 90, 0, (s) => s.environment.tas_m_s),
  sig("airflow", "Air flow", "kg/s", "lb/s", (v) => v * 2.20462, 0, 0.2, 3, (s) => s.air_flow_kg_s),
];

const piston: KindDef = {
  kind: "piston",
  title: "Piston",
  eicasTitle: "Engine instruments (GA panel)",
  primary: [
    { label: "RPM", unit: "", min: 0, max: 3000, redline: 2700, green: [2100, 2700], decimals: 0, get: (s) => s.rpm },
    { label: "MAN PRESS", unit: "inHg", min: 0, max: 32, decimals: 1, get: (s) => s.map_inhg },
    { label: "EGT", unit: "°F", min: 0, max: 1700, decimals: 0, get: (s) => s.egt_f },
    { label: "CHT", unit: "°F", min: 0, max: 550, redline: 500, amber: [435, 500], green: [200, 435], decimals: 0, get: (s) => s.cht_f },
  ],
  secondary: [
    { label: "OIL PRESS", unit: "psi", min: 0, max: 120, redlineLow: 25, redline: 115, green: [55, 95], amber: [25, 55], decimals: 0, get: (s) => s.oil_pressure_psi },
    { label: "OIL TEMP", unit: "°F", min: 0, max: 260, redline: 245, green: [140, 245], decimals: 0, get: (s) => s.oil_temperature_f },
    { label: "FUEL FLOW", unit: "gal/h", min: 0, max: 20, green: [0, 15], decimals: 1, get: (s) => s.fuel_flow_gph },
    { label: "FUEL PRESS", unit: "psi", min: 0, max: 10, redlineLow: 0.5, redline: 8, green: [0.5, 8], decimals: 1, get: (s) => s.fuel_pressure_psi },
  ],
  controls: [
    { type: "select", name: "magnetos", label: "Magnetos", options: [[0, "OFF"], [1, "L"], [2, "R"], [3, "BOTH"]], get: (s) => n(s.controls.magnetos) },
    { type: "toggle", name: "starter", label: "STARTER", on: "CRANK", off: "OFF", get: (s) => b(s.controls.starter) },
    { type: "momentary", name: "primer", label: "PRIMER", on: "shot", off: "shot", get: (s) => n(s.controls.primer_shots), status: (s) => `${n(s.controls.primer_shots)} shots${s.primed ? " · primed" : ""}` },
    { type: "slider", name: "throttle", label: "Throttle", min: 0, max: 1, step: 0.005, scale: 100, get: (s) => n(s.controls.throttle), presets: [[0, "Idle"], [0.45, "Run-up"], [0.75, "Cruise"], [1, "Full"]], width: 3 },
    { type: "slider", name: "mixture", label: "Mixture (0 = idle cutoff)", min: 0, max: 1, step: 0.005, scale: 100, get: (s) => n(s.controls.mixture), presets: [[1, "Full rich"], [0.72, "Lean cruise"], [0.55, "Best power"], [0, "Cutoff"]], width: 3 },
    { type: "toggle", name: "carb_heat", label: "CARB HEAT", on: "HOT", off: "COLD", get: (s) => b(s.controls.carb_heat) },
    { type: "toggle", name: "fuel_pump", label: "FUEL PUMP", on: "ON", off: "OFF", get: (s) => b(s.controls.fuel_pump) },
    { type: "select", name: "fuel_selector", label: "Fuel selector", options: [[0, "OFF"], [1, "LEFT"], [2, "RIGHT"], [3, "BOTH"]], get: (s) => n(s.controls.fuel_selector) },
  ],
  faults: [
    { key: "mag_left_fail", label: "Left magneto dead", kind: "bool" },
    { key: "mag_right_fail", label: "Right magneto dead", kind: "bool" },
    { key: "plug_fouled", label: "Fouled plug (cyl 3, left mag)", kind: "bool" },
    { key: "cylinder_dead", label: "Dead cylinder (stuck valve)", kind: "bool" },
    { key: "oil_leak_qt_per_min", label: "Oil leak, qt/min", kind: "num", def: 0, step: 0.1 },
    { key: "vapor_lock", label: "Vapour lock (hot start)", kind: "bool" },
    { key: "induction_fire", label: "Induction fire", kind: "bool" },
    { key: "carb_ice_enabled", label: "Carburettor icing possible", kind: "bool" },
    { key: "starter_inop", label: "Starter inoperative", kind: "bool" },
  ],
  signals: pistonSignals,
  mainLever: "throttle",
  fuelControl: { name: "mixture", on: 1, off: 0 },
  runningPrep: [["magnetos", 3], ["starter", 0], ["mixture", 1]],
  idleLevel: 0, cruiseLevel: 0.75, cruiseEnv: [2438, 0.18],
  modeOf: (s) => s.mode,
  readouts: (s) => [
    ["Brake power", `${s.power_hp.toFixed(1)} hp (${s.power_pct.toFixed(0)} %)`],
    ["Indicated / friction", `${(s.indicated_power_w / 745.7).toFixed(1)} / ${(s.friction_power_w / 745.7).toFixed(1)} hp`],
    ["Torque", `${s.torque_nm.toFixed(0)} N·m`],
    ["Fuel flow", `${s.fuel_flow_gph.toFixed(2)} gal/h · ${fmtFuelFlow(s.fuel_flow_kg_s)}`],
    ["Fuel/air ratio", `${s.fuel_air_ratio.toFixed(4)} (stoich 0.0667)`, s.fuel_air_ratio < 0.06],
    ["BSFC", `${s.bsfc_lb_hp_h.toFixed(2)} lb/hp·h`],
    ["Thermal efficiency", `${(s.thermal_efficiency * 100).toFixed(1)} %`],
    ["Air flow", fmtMassFlow(s.air_flow_kg_s)],
    ["Fuel used / tanks", `${s.fuel_used_gal.toFixed(2)} gal · L ${n(s.controls.fuel_left_gal).toFixed(1)} / R ${n(s.controls.fuel_right_gal).toFixed(1)} gal`],
    ["Prop thrust", fmtThrust(s.thrust_n)],
    ["Prop: β / α / J / η", `${s.prop.beta_deg.toFixed(1)}° / ${s.prop.alpha_deg.toFixed(1)}° / ${s.prop.advance_ratio.toFixed(2)} / ${(s.prop.efficiency * 100).toFixed(0)} %${s.prop.stalled ? " · stalled" : ""}`],
    ["Tip Mach", s.prop.tip_mach.toFixed(2), s.prop.tip_mach > 0.9],
    ["Carb ice / induction temp", `${(s.carb_ice * 100).toFixed(0)} % / ${s.induction_temp_c.toFixed(0)} °C`, s.carb_ice > 0.15],
    ["Cylinders firing", `${s.cylinders_firing} / 4`, s.firing && s.cylinders_firing < 4],
    ["Roughness", s.roughness.toFixed(2), s.roughness > 0.4],
    ["Oil quantity", `${s.oil_quantity_qt.toFixed(1)} qt`],
    ["OAT / altitude / TAS", `${fmtTemp(s.ambient.temperature_k)} / ${fmtAlt(s.environment.altitude_m)} / ${fmtSpeed(s.environment.tas_m_s)}`],
    ["Humidity", `${(s.environment.humidity * 100).toFixed(0)} %`],
    ["Run time", `${s.run_time_s.toFixed(0)} s`],
  ],
  stations: (s) => [
    { id: "amb", label: "Ambient", tt_k: s.ambient.temperature_k, pt_pa: s.ambient.pressure_pa, w_kg_s: 0 },
    { id: "ind", label: "Induction (after carb heat)", tt_k: s.induction_temp_c + 273.15, pt_pa: s.ambient.pressure_pa, w_kg_s: s.air_flow_kg_s },
    { id: "man", label: "Manifold", tt_k: s.induction_temp_c + 273.15, pt_pa: s.map_pa, w_kg_s: s.air_flow_kg_s },
    { id: "exh", label: "Exhaust", tt_k: s.egt_c + 273.15, pt_pa: s.ambient.pressure_pa, w_kg_s: s.air_flow_kg_s + s.fuel_flow_kg_s },
  ],
  lessons: [
    ["piston-01-cold-start.txt", "P01 Cold start, warm-up, run-up"],
    ["piston-02-mag-check.txt", "P02 Magneto check and fouled plug"],
    ["piston-03-takeoff-climb.txt", "P03 Takeoff, climb, temperatures"],
    ["piston-04-leaning.txt", "P04 Leaning at altitude: peak EGT"],
    ["piston-05-carb-ice.txt", "P05 Carburettor icing"],
    ["piston-06-fuel-starvation.txt", "P06 Fuel starvation and restart"],
  ],
  defaultCharts: ["rpm", "egt", "cht", "map"],
};

// --------------------------------------------------------------------------
// Turboprop: PT6A-114A
// --------------------------------------------------------------------------
const turbopropSignals: Signal[] = [
  sig("torque", "Torque", "ft·lb", "ft·lb", id, 0, 2200, 0, (s) => (s.torque_valid ? s.torque_ftlb : NaN), [/^torque/i, /^trq/i, /^tq/i], U_TORQUE),
  sig("itt", "ITT", "°C", "°C", id, 0, 1200, 0, (s) => (s.itt_valid ? s.itt_c : NaN), [/^itt/i, /^egt/i], U_TEMP_C),
  sig("ng", "Ng", "%", "%", id, 0, 110, 1, (s) => s.ng_pct, [/^ng/i, /^n1/i, /^nh/i], U_PCT),
  sig("np", "Np", "rpm", "rpm", id, 0, 2200, 0, (s) => s.np_rpm, [/^np/i, /^prop.?rpm/i, /^rpm/i], U_RPM),
  sig("ff", "Fuel flow", "kg/s", "lb/h", (v) => v * 7936.64, 0, 0.08, 4, (s) => s.fuel_flow_kg_s, [/^fuel_flow/i, /^ff$/i, /fuel.?flow/i], U_FF),
  sig("ff_pph", "Fuel flow", "lb/h", "lb/h", id, 0, 500, 0, (s) => s.fuel_flow_pph),
  sig("shp", "Shaft power", "hp", "hp", id, 0, 750, 0, (s) => s.shaft_power_hp, [/^shp/i, /shaft.?power/i, /^power/i], U_HP),
  sig("oilp", "Oil pressure", "psi", "psi", id, 0, 120, 0, (s) => s.oil_pressure_psi, [/oil.?press/i, /^oilp/i], U_PSI),
  sig("oilt", "Oil temperature", "°C", "°C", id, -40, 120, 0, (s) => s.oil_temperature_c, [/oil.?temp/i, /^oilt/i], U_TEMP_C),
  sig("t4", "T4 turbine inlet", "K", "K", id, 200, 1600, 0, (s) => s.t4_k),
  sig("beta", "Blade angle", "°", "°", id, -15, 90, 1, (s) => s.prop_beta_deg),
  sig("thrust", "Thrust", "kN", "lbf", (v) => v * 224.809, -8, 14, 2, (s) => s.thrust_n / 1000),
  sig("eta_p", "Prop efficiency", "", "", id, 0, 1, 2, (s) => s.prop.efficiency),
  sig("sfc", "SFC", "lb/shp·h", "lb/shp·h", id, 0, 1.5, 2, (s) => s.sfc_lb_shp_h),
  sig("wf_p3", "Wf/Ps3", "(kg/h)/kPa", "(kg/h)/kPa", id, 0, 0.5, 3, (s) => s.wf_p3_ratio),
  sig("ff_cmd", "Fuel command", "kg/s", "lb/h", (v) => v * 7936.64, 0, 0.1, 4, (s) => s.fuel_command_kg_s),
  sig("ff_max", "Accel limit", "kg/s", "lb/h", (v) => v * 7936.64, 0, 0.1, 4, (s) => s.fuel_accel_limit_kg_s),
  sig("sm", "Surge margin", "%", "%", id, -10, 25, 1, (s) => s.surge_margin_pct),
  sig("vib", "Vibration", "units", "units", id, 0, 5, 1, (s) => s.vib),
  sig("power_lever", "Power lever", "", "", id, -0.3, 1, 2, (s) => n(s.controls.power_lever)),
  sig("prop_lever", "Prop lever", "", "", id, 0, 1, 2, (s) => n(s.controls.prop_lever)),
  sig("alt", "Altitude", "m", "ft", (v) => v / 0.3048, 0, 8000, 0, (s) => s.environment.altitude_m),
  sig("mach", "Mach", "", "", id, 0, 0.5, 2, (s) => s.environment.mach),
  sig("opr", "Compressor PR", "", "", id, 0, 12, 2, (s) => s.cycle.pr),
  sig("w2", "Air flow", "kg/s", "lb/s", (v) => v * 2.20462, 0, 4, 2, (s) => s.cycle.w2),
];

const turboprop: KindDef = {
  kind: "turboprop",
  title: "Turboprop",
  eicasTitle: "Engine indications (Caravan panel)",
  primary: [
    { label: "TORQUE", unit: "ft·lb", min: 0, max: 2200, redline: 1970, green: [0, 1970], decimals: 0, get: (s) => s.torque_ftlb, valid: (s) => s.torque_valid },
    { label: "ITT", unit: "°C", min: 0, max: 1200, redline: 805, amber: [765, 805], decimals: 0, get: (s) => s.itt_c, valid: (s) => s.itt_valid },
    { label: "Ng", unit: "%", min: 0, max: 110, redline: 101.6, decimals: 1, get: (s) => s.ng_pct },
    { label: "PROP", unit: "rpm", min: 0, max: 2200, redline: 1900, green: [1600, 1900], decimals: 0, bug: true, get: (s) => s.np_rpm, bugOf: (s) => s.np_target_rpm },
  ],
  secondary: [
    { label: "FUEL FLOW", unit: "lb/h", min: 0, max: 500, decimals: 0, get: (s) => s.fuel_flow_pph },
    { label: "OIL PRESS", unit: "psi", min: 0, max: 120, redlineLow: 40, amber: [40, 85], green: [85, 105], decimals: 0, get: (s) => s.oil_pressure_psi },
    { label: "OIL TEMP", unit: "°C", min: -40, max: 120, redline: 99, green: [10, 99], decimals: 0, get: (s) => s.oil_temperature_c },
    { label: "SHP", unit: "hp", min: 0, max: 750, decimals: 0, digitalOnly: true, get: (s) => s.shaft_power_hp },
  ],
  controls: [
    { type: "toggle", name: "starter", label: "STARTER", on: "ON", off: "OFF", get: (s) => b(s.controls.starter), status: (s) => (s.starter_engaged ? "MOTORING" : s.controls.starter ? "ON" : "OFF") },
    { type: "toggle", name: "ignition", label: "IGNITION", on: "ON", off: "AUTO", get: (s) => b(s.controls.ignition) },
    { type: "select", name: "condition_lever", label: "Condition lever", options: [[0, "CUTOFF"], [1, "LOW IDLE"], [2, "HIGH IDLE"]], get: (s) => n(s.controls.condition_lever) },
    { type: "slider", name: "power_lever", label: "Power lever (reverse … beta … max)", min: -0.3, max: 1, step: 0.005, scale: 100, get: (s) => n(s.controls.power_lever), presets: [[-0.3, "Max rev"], [0, "Beta/idle"], [0.5, "Cruise"], [1, "Max"]], width: 3 },
    { type: "slider", name: "prop_lever", label: "Prop lever (feather … 1600 … 1900 rpm)", min: 0, max: 1, step: 0.005, scale: 100, get: (s) => n(s.controls.prop_lever), presets: [[0, "Feather"], [0.1, "1600"], [0.6, "1750"], [1, "1900"]], width: 3 },
    { type: "toggle", name: "bleed_air", label: "BLEED AIR", on: "ON", off: "OFF", get: (s) => b(s.controls.bleed_air) },
    { type: "toggle", name: "inertial_separator", label: "INERTIAL SEP", on: "BYPASS", off: "NORMAL", get: (s) => b(s.controls.inertial_separator) },
  ],
  faults: [
    { key: "starter_inop", label: "Starter-generator inoperative", kind: "bool" },
    { key: "starter_weak", label: "Weak battery (hot start risk)", kind: "bool" },
    { key: "starter_early_cutout", label: "Starter drops out early (hung start)", kind: "bool" },
    { key: "ignition_fail", label: "Igniters failed", kind: "bool" },
    { key: "start_fuel_factor", label: "Start fuel schedule × (1.7 = hot start)", kind: "num", def: 1, step: 0.1 },
    { key: "fuel_pump_fail", label: "Fuel supply failure (flameout)", kind: "bool" },
    { key: "oil_leak_qt_per_min", label: "Oil leak, qt/min", kind: "num", def: 0, step: 0.2 },
    { key: "fire", label: "Engine fire", kind: "bool" },
    { key: "prop_governor_fail", label: "Prop governor failed (blade angle frozen)", kind: "bool" },
    { key: "compressor_damage_pct", label: "Surge margin lost, % points", kind: "num", def: 0, step: 1 },
    { key: "fod", label: "Foreign object damage", kind: "bool" },
    { key: "itt_probe_fail", label: "ITT harness open", kind: "bool" },
    { key: "torque_sensor_fail", label: "Torque indication failed", kind: "bool" },
    { key: "chip_detector", label: "Chip detector light", kind: "bool" },
    { key: "trigger_surge", label: "Surge now", kind: "bool", oneshot: true },
    { key: "trigger_flameout", label: "Flameout now", kind: "bool", oneshot: true },
    { key: "fire_bottle", label: "Fire bottle", kind: "bool", oneshot: true },
  ],
  signals: turbopropSignals,
  mainLever: "power_lever",
  fuelControl: { name: "condition_lever", on: 1, off: 0 },
  runningPrep: [["condition_lever", 1], ["prop_lever", 1], ["starter", 0]],
  idleLevel: 0, cruiseLevel: 1.0, cruiseEnv: [3048, 0.25],
  modeOf: (s) => s.mode,
  readouts: (s) => {
    const c = s.cycle;
    return [
      ["Shaft power", `${s.shaft_power_hp.toFixed(0)} shp (${s.power_pct.toFixed(0)} %)`],
      ["Torque", `${s.torque_ftlb.toFixed(0)} ft·lb · ${s.torque_nm.toFixed(0)} N·m (${s.torque_pct.toFixed(0)} %)`, s.torque_pct > 100],
      ["Np / target", `${s.np_rpm.toFixed(0)} / ${s.np_target_rpm.toFixed(0)} rpm${s.governing ? " · governing" : s.feathered ? " · feathered" : " · fine stop"}`],
      ["Blade angle β", `${s.prop_beta_deg.toFixed(1)}°`],
      ["Thrust (prop + jet)", `${fmtThrust(s.thrust_n)} (jet ${fmtThrust(s.jet_thrust_n)})`],
      ["Prop: α / J / η", `${s.prop.alpha_deg.toFixed(1)}° / ${s.prop.advance_ratio.toFixed(2)} / ${(s.prop.efficiency * 100).toFixed(0)} %${s.prop.stalled ? " · stalled" : ""}`],
      ["Fuel flow", `${s.fuel_flow_pph.toFixed(0)} lb/h · ${fmtFuelFlow(s.fuel_flow_kg_s)}`],
      ["Fuel: steady / demand", `${(s.fuel_flow_steady_kg_s * 7936.64).toFixed(0)} / ${(s.fuel_command_kg_s * 7936.64).toFixed(0)} lb/h`],
      ["Fuel limits (decel / accel)", `${(s.fuel_decel_limit_kg_s * 7936.64).toFixed(0)} / ${(s.fuel_accel_limit_kg_s * 7936.64).toFixed(0)} lb/h`],
      ["Wf/Ps3", `${s.wf_p3_ratio.toFixed(3)} (kg/h)/kPa · Ps3 ${fmtPressure(s.ps3_kpa * 1000)}`],
      ["SFC", `${s.sfc_lb_shp_h.toFixed(2)} lb/shp·h`],
      ["Fuel used", fmtMass(s.fuel_used_kg)],
      ["Ng command / idle", `${s.ng_command_pct.toFixed(1)} / ${s.ng_idle_pct.toFixed(1)} %`],
      ["Ng / corrected", `${s.ng_rpm.toFixed(0)} rpm / ${s.ng_corrected_pct.toFixed(1)} %`],
      ["T4 actual / steady", `${s.t4_k.toFixed(0)} / ${s.t4_steady_k.toFixed(0)} K`, Math.abs(s.t4_k - s.t4_steady_k) > 50],
      ["ITT gas (true)", `${s.itt_gas_c.toFixed(0)} °C`],
      ["Compressor PR / T3", `${c.pr.toFixed(2)} / ${fmtTemp(c.tt3)}`],
      ["CT PR / PT PR", `${c.ct_pr.toFixed(2)} / ${c.pt_pr.toFixed(2)}`],
      ["Air flow", fmtMassFlow(c.w2)],
      ["Surge margin", `${s.surge_margin_pct.toFixed(1)} %`, s.surge_margin_pct < 6],
      ["Thermal efficiency", `${(c.thermal_efficiency * 100).toFixed(1)} %`],
      ["Oil quantity", `${s.oil_quantity_qt.toFixed(1)} qt`],
      ["OAT / altitude / TAS", `${fmtTemp(s.inlet.ambient.temperature_k)} / ${fmtAlt(s.environment.altitude_m)} / ${fmtSpeed(s.inlet.true_airspeed_m_s)}`],
      ["Surges / flameouts", `${s.surge_count} / ${s.flameout_count}`, s.surge_count + s.flameout_count > 0],
      ["Run time", `${s.run_time_s.toFixed(0)} s`],
    ];
  },
  stations: (s) => {
    const c = s.cycle;
    return [
      { id: "2", label: "Compressor inlet", tt_k: s.inlet.tt2_k, pt_pa: s.inlet.pt2_pa, w_kg_s: c.w2 },
      { id: "3", label: "Compressor exit", tt_k: c.tt3, pt_pa: c.pt3, w_kg_s: c.w2 },
      { id: "4", label: "Combustor exit / CT inlet", tt_k: c.t4, pt_pa: c.pt4, w_kg_s: c.w2 + c.wf },
      { id: "45", label: "CT exit / PT inlet (ITT)", tt_k: c.tt45, pt_pa: c.pt45, w_kg_s: c.w2 + c.wf },
      { id: "5", label: "PT exit", tt_k: c.tt5, pt_pa: c.pt5, w_kg_s: c.w2 + c.wf },
      { id: "9", label: "Exhaust stubs", tt_k: c.tt5, pt_pa: s.inlet.ambient.pressure_pa, w_kg_s: c.w2 + c.wf },
    ];
  },
  lessons: [
    ["turboprop-01-start.txt", "T01 Start: motoring, light-off, idle"],
    ["turboprop-02-takeoff-torque-itt.txt", "T02 Takeoff: torque vs ITT limits"],
    ["turboprop-03-prop-lever.txt", "T03 Prop governing and feather"],
    ["turboprop-04-hot-start.txt", "T04 Hot start on a weak battery"],
    ["turboprop-05-reverse.txt", "T05 Landing roll and reverse"],
    ["turboprop-06-flameout.txt", "T06 Flameout and relight"],
  ],
  defaultCharts: ["torque", "itt", "ng", "np"],
};

export const KINDS: Record<string, KindDef> = { turbofan, piston, turboprop };

export function kindDef(kind: string): KindDef {
  return KINDS[kind] ?? turbofan;
}

void unitSystem;
