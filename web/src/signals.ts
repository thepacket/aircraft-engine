// Central registry of plottable / comparable signals. Values are always SI in
// the state; the registry knows how to present each one in both unit systems.
import type { EngineState } from "./types";

export interface Signal {
  key: string;
  label: string;
  unitSi: string;
  unitImp: string;
  /** SI -> imperial conversion (identity when both are the same) */
  toImp: (v: number) => number;
  /** chart ranges in SI */
  min: number;
  max: number;
  decimals: number;
  get: (s: EngineState) => number;
}

const id = (v: number): number => v;
const sig = (key: string, label: string, unitSi: string, unitImp: string, toImp: (v: number) => number, min: number, max: number, decimals: number, get: (s: EngineState) => number): Signal => ({ key, label, unitSi, unitImp, toImp, min, max, decimals, get });

export const SIGNALS: Signal[] = [
  sig("n1", "N1", "%", "%", id, 0, 110, 1, (s) => s.n1_pct),
  sig("n2", "N2", "%", "%", id, 0, 110, 1, (s) => s.n2_pct),
  sig("n1c", "N1 corrected", "%", "%", id, 0, 110, 1, (s) => s.n1_corrected_pct),
  sig("n1_cmd", "N1 command", "%", "%", id, 0, 110, 1, (s) => s.n1_command_pct),
  sig("egt", "EGT", "°C", "°C", id, 0, 1000, 0, (s) => (s.egt_valid ? s.egt_c : NaN)),
  sig("egt_gas", "EGT gas (true)", "°C", "°C", id, 0, 1000, 0, (s) => s.egt_gas_c),
  sig("t4", "T4 turbine inlet", "K", "K", id, 200, 2000, 0, (s) => s.t4_k),
  sig("ff", "Fuel flow", "kg/s", "lb/h", (v) => v * 7936.64, 0, 1.5, 3, (s) => s.fuel_flow_kg_s),
  sig("ff_cmd", "Fuel command", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_command_kg_s),
  sig("ff_max", "Accel limit", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_accel_limit_kg_s),
  sig("ff_min", "Decel limit", "kg/s", "lb/h", (v) => v * 7936.64, 0, 3, 3, (s) => s.fuel_decel_limit_kg_s),
  sig("wf_p3", "Wf/Ps3", "(kg/h)/kPa", "(kg/h)/kPa", id, 0, 3, 2, (s) => s.wf_p3_ratio),
  sig("ps3", "Ps3 burner pressure", "kPa", "psi", (v) => v / 6.89476, 0, 3500, 0, (s) => s.ps3_kpa),
  sig("thrust", "Net thrust", "kN", "lbf", (v) => v * 224.809, -40, 130, 1, (s) => s.thrust_n / 1000),
  sig("tsfc", "TSFC", "lb/lbf·h", "lb/lbf·h", id, 0, 1.5, 3, (s) => s.tsfc_lb_lbf_h),
  sig("oilp", "Oil pressure", "psi", "psi", id, 0, 100, 0, (s) => s.oil_pressure_psi),
  sig("oilt", "Oil temperature", "°C", "°C", id, -40, 180, 0, (s) => s.oil_temperature_c),
  sig("oilq", "Oil quantity", "qt", "qt", id, 0, 22, 1, (s) => s.oil_quantity_qt),
  sig("vib", "Vibration", "units", "units", id, 0, 5, 1, (s) => Math.max(s.vib_n1, s.vib_n2)),
  sig("sm_hpc", "HPC surge margin", "%", "%", id, -10, 30, 1, (s) => s.hpc_surge_margin_pct),
  sig("sm_fan", "Fan surge margin", "%", "%", id, -10, 30, 1, (s) => s.fan_surge_margin_pct),
  sig("vbv", "VBV position", "open", "open", id, 0, 1, 2, (s) => s.vbv_open),
  sig("vsv", "VSV angle", "°", "°", id, 0, 45, 0, (s) => s.vsv_angle_deg),
  sig("bleed", "Customer bleed", "kg/s", "lb/s", (v) => v * 2.20462, 0, 6, 2, (s) => s.cycle.bleed_kg_s),
  sig("rev", "Reverser position", "", "", id, 0, 1, 2, (s) => s.reverser_position),
  sig("tla", "Thrust lever", "", "", id, 0, 1, 2, (s) => s.controls.tla),
  sig("alt", "Altitude", "m", "ft", (v) => v / 0.3048, 0, 13000, 0, (s) => s.environment.altitude_m),
  sig("mach", "Mach", "", "", id, 0, 1, 2, (s) => s.environment.mach),
  sig("opr", "Overall PR", "", "", id, 0, 40, 1, (s) => s.cycle.overall_pressure_ratio),
  sig("w2", "Inlet mass flow", "kg/s", "lb/s", (v) => v * 2.20462, 0, 400, 1, (s) => s.cycle.w2_kg_s),
  sig("bpr", "Bypass ratio", "", "", id, 0, 10, 2, (s) => s.cycle.bypass_ratio),
  sig("eff_th", "Thermal efficiency", "%", "%", id, 0, 60, 1, (s) => s.cycle.thermal_efficiency * 100),
  sig("eff_pr", "Propulsive efficiency", "%", "%", id, 0, 100, 1, (s) => s.cycle.propulsive_efficiency * 100),
];

const BY_KEY = new Map(SIGNALS.map((s) => [s.key, s]));
export function signal(key: string): Signal {
  const s = BY_KEY.get(key);
  if (!s) throw new Error(`unknown signal ${key}`);
  return s;
}
export function hasSignal(key: string): boolean {
  return BY_KEY.has(key);
}
