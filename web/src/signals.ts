// Signal registry: the plottable / comparable channels of the active engine
// kind. Values are SI in the state; each signal knows its imperial form, its
// chart range, and (for log import) the column-name patterns and unit options.
import type { AnyState } from "./types";

export interface ImportUnit { label: string; toSi: (v: number) => number }

export interface Signal {
  key: string;
  label: string;
  unitSi: string;
  unitImp: string;
  toImp: (v: number) => number;
  min: number;
  max: number;
  decimals: number;
  get: (s: AnyState) => number;
  /** column-name patterns for flight-log import; absent = not importable */
  patterns?: RegExp[];
  importUnits?: ImportUnit[];
}

export const id = (v: number): number => v;

export function sig(key: string, label: string, unitSi: string, unitImp: string, toImp: (v: number) => number, min: number, max: number, decimals: number, get: (s: AnyState) => number, patterns?: RegExp[], importUnits?: ImportUnit[]): Signal {
  return { key, label, unitSi, unitImp, toImp, min, max, decimals, get, patterns, importUnits };
}

let active: Signal[] = [];
let byKey = new Map<string, Signal>();

export function setActiveSignals(list: Signal[]): void {
  active = list;
  byKey = new Map(list.map((s) => [s.key, s]));
}
export function activeSignals(): Signal[] { return active; }
export function signal(key: string): Signal {
  const s = byKey.get(key);
  if (!s) throw new Error(`unknown signal ${key}`);
  return s;
}
export function hasSignal(key: string): boolean { return byKey.has(key); }

// Common unit option sets
export const U_FF: ImportUnit[] = [{ label: "kg/s", toSi: id }, { label: "kg/h", toSi: (v) => v / 3600 }, { label: "lb/h", toSi: (v) => v / 7936.64 }, { label: "gal/h", toSi: (v) => v * 0.72 * 3.78541 / 3600 }];
export const U_TEMP_C: ImportUnit[] = [{ label: "°C", toSi: id }, { label: "K", toSi: (v) => v - 273.15 }, { label: "°F", toSi: (v) => (v - 32) / 1.8 }];
export const U_TEMP_F: ImportUnit[] = [{ label: "°F", toSi: id }, { label: "°C", toSi: (v) => v * 1.8 + 32 }];
export const U_THRUST_KN: ImportUnit[] = [{ label: "kN", toSi: id }, { label: "N", toSi: (v) => v / 1000 }, { label: "lbf", toSi: (v) => v * 4.448222 / 1000 }];
export const U_PCT: ImportUnit[] = [{ label: "%", toSi: id }];
export const U_RPM: ImportUnit[] = [{ label: "rpm", toSi: id }];
export const U_PSI: ImportUnit[] = [{ label: "psi", toSi: id }, { label: "kPa", toSi: (v) => v / 6.89476 }];
export const U_INHG: ImportUnit[] = [{ label: "inHg", toSi: id }, { label: "kPa", toSi: (v) => v / 3.38639 }];
export const U_TORQUE: ImportUnit[] = [{ label: "ft·lb", toSi: id }, { label: "N·m", toSi: (v) => v / 1.355818 }, { label: "%", toSi: (v) => v * 19.7 }];
export const U_HP: ImportUnit[] = [{ label: "hp", toSi: id }, { label: "kW", toSi: (v) => v / 0.7457 }];
