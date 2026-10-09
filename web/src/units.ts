// Unit system toggle. State values are SI; presentation converts.
export type UnitSystem = "si" | "imp";

let system: UnitSystem = "si";
export function unitSystem(): UnitSystem { return system; }
export function setUnitSystem(u: UnitSystem): void { system = u; }

export const FT = 0.3048;
export const LBF = 4.448222;
export const PPH = 7936.64; // kg/s -> lb/h
export const KT = 1.94384;

export function fmtThrust(n: number): string {
  return system === "si" ? `${(n / 1000).toFixed(1)} kN` : `${(n / LBF).toFixed(0)} lbf`;
}
export function fmtFuelFlow(kgs: number): string {
  return system === "si" ? `${kgs.toFixed(3)} kg/s` : `${(kgs * PPH).toFixed(0)} lb/h`;
}
export function fmtMass(kg: number): string {
  return system === "si" ? `${kg.toFixed(1)} kg` : `${(kg * 2.20462).toFixed(0)} lb`;
}
export function fmtAlt(m: number): string {
  return system === "si" ? `${m.toFixed(0)} m` : `${(m / FT).toFixed(0)} ft`;
}
export function fmtSpeed(ms: number): string {
  return system === "si" ? `${ms.toFixed(0)} m/s` : `${(ms * KT).toFixed(0)} kt`;
}
export function fmtPressure(pa: number): string {
  return system === "si" ? `${(pa / 1000).toFixed(1)} kPa` : `${(pa / 6894.76).toFixed(1)} psi`;
}
export function fmtMassFlow(kgs: number): string {
  return system === "si" ? `${kgs.toFixed(1)} kg/s` : `${(kgs * 2.20462).toFixed(1)} lb/s`;
}
export function fmtPower(w: number): string {
  return system === "si" ? `${(w / 1e6).toFixed(2)} MW` : `${(w / 745.7).toFixed(0)} hp`;
}
export function fmtTemp(k: number): string {
  return system === "si" ? `${(k - 273.15).toFixed(0)} °C` : `${((k - 273.15) * 1.8 + 32).toFixed(0)} °F`;
}
