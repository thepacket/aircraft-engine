//! Single-spool afterburning turbojet: General Electric J85-GE-5A (2,680 lbf
//! dry, 3,850 lbf in afterburner) as fitted to the Northrop T-38A Talon.
//!
//! One spool: an 8-stage axial compressor driven by a 2-stage turbine. The
//! exhaust passes through an afterburner (reheat) duct into a variable-area
//! convergent nozzle. The nozzle area sets the turbine operating line: it is
//! open at idle, closes toward military power to raise the turbine
//! temperature and thrust, and opens again when the afterburner lights so
//! that the reheated gas passes without back-pressuring the turbine. EGT is
//! measured at the turbine exit (station 5).
//!
//! Sources: FAA / USAF T-38A flight manual (limits, indications, start),
//! GE J85 published performance (rating, airflow, pressure ratio, weight).
//! Component characteristics and the fuel-control schedules are calibrated
//! approximations, as for the other engine kinds.

use crate::atmosphere::{inlet as inlet_conditions, isa, Inlet};
use crate::cycle::{burner_fuel, burner_t_out, nozzle, CP_AIR, CP_GAS, GAMMA_GAS, WINDMILL_PT_LOSS};
use crate::logger::DataLogger;
use crate::spec::interp;
use serde::{Deserialize, Serialize};

const TAU: f64 = std::f64::consts::TAU;
const LBF: f64 = 4.448222;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbojetSpec {
    pub name: String,
    pub manufacturer: String,
    pub application: String,
    pub rating: TjRating,
    pub geometry: TjGeometry,
    pub design_point: TjDesign,
    pub control: TjControl,
    pub start: TjStart,
    pub oil: TjOil,
    pub limits: TjLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjRating {
    /// Sea-level static military (dry) thrust, ISA (N)
    pub thrust_dry_n: f64,
    /// Sea-level static maximum afterburning thrust, ISA (N)
    pub thrust_ab_n: f64,
    /// Spool speed at 100 % (rpm)
    pub n_100pct_rpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjGeometry {
    pub length_m: f64,
    pub diameter_m: f64,
    pub dry_mass_kg: f64,
    pub compressor_stages: u32,
    pub turbine_stages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjDesign {
    /// Inlet mass flow at 100 % corrected speed, SL static (kg/s)
    pub mass_flow_kg_s: f64,
    pub compressor_pressure_ratio: f64,
    pub eta_compressor: f64,
    pub eta_burner: f64,
    pub eta_turbine: f64,
    pub eta_mechanical: f64,
    pub burner_pressure_loss: f64,
    pub cooling_bleed_fraction: f64,
    pub power_extraction_w: f64,
    pub fuel_lhv_j_kg: f64,
    /// Compressor characteristic exponents vs corrected speed
    pub flow_exponent: f64,
    pub pr_exponent: f64,
    pub efficiency_falloff: f64,
    /// Afterburner: combustion efficiency, duct total pressure loss dry and
    /// lit, and the maximum reheat temperature (K) at full afterburner
    pub ab_efficiency: f64,
    pub ab_pressure_loss_dry: f64,
    pub ab_pressure_loss_lit: f64,
    pub ab_t7_max_k: f64,
    /// Nozzle discharge coefficient
    pub nozzle_cd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjControl {
    /// Ground idle (corrected) speed (%) and its increase with altitude (%/km)
    pub n_idle_pct: f64,
    pub n_idle_per_km: f64,
    /// Throttle (0..1 idle..military) to speed mapping exponent
    pub lever_exponent: f64,
    /// Speed governor gain (kg/s of fuel per % speed error)
    pub governor_gain_kg_s_per_pct: f64,
    /// Acceleration / deceleration limits as Wf/Ps3 [(kg/h)/kPa] vs corrected N (%)
    pub accel_wf_p3_vs_nc: Vec<[f64; 2]>,
    pub decel_wf_p3_vs_nc: Vec<[f64; 2]>,
    pub min_fuel_kg_s: f64,
    pub fuel_lag_s: f64,
    /// Nozzle area schedule, as fractions of the military area: open at
    /// idle, closed (1.0) at military, open again at full afterburner
    pub nozzle_idle_frac: f64,
    pub nozzle_ab_max_frac: f64,
    /// Nozzle actuator rate (fraction of military area per second)
    pub nozzle_rate_per_s: f64,
    /// Afterburner: minimum speed for a light (%), light delay (s), fuel ramp time (s)
    pub ab_min_n_pct: f64,
    pub ab_light_delay_s: f64,
    pub ab_ramp_s: f64,
    /// Spool polar inertia (kg m^2)
    pub inertia_kg_m2: f64,
    /// Surge margin at design (%)
    pub surge_margin_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjStart {
    pub starter_max_n_pct: f64,
    pub starter_tau_s: f64,
    pub min_light_n_pct: f64,
    pub light_off_delay_s: f64,
    pub starter_cutout_n_pct: f64,
    pub max_light_altitude_m: f64,
    pub windmill_n_per_mach: f64,
    /// Sub-idle fuel schedule [N %, kg/s], scaled to meet the idle steady fuel
    pub start_fuel_schedule: Vec<[f64; 2]>,
    /// Gross sub-idle acceleration from the turbine (%/s) vs N (%)
    pub start_accel_vs_n: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjOil {
    pub capacity_qt: f64,
    pub full_qt: f64,
    pub press_gain_psi_per_pct: f64,
    pub press_offset_psi: f64,
    pub temp_rise_idle_k: f64,
    pub temp_rise_mil_k: f64,
    pub temp_tau_s: f64,
    pub starvation_qt: f64,
    pub seizure_time_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjLimits {
    pub n_max_pct: f64,
    /// EGT (turbine exit) limits (C): military continuous, transient / afterburner, start
    pub egt_max_c: f64,
    pub egt_transient_c: f64,
    pub egt_start_c: f64,
    pub oil_press_min_psi: f64,
    pub oil_press_min_idle_psi: f64,
    pub oil_press_max_psi: f64,
    pub oil_temp_max_c: f64,
    /// Afterburner time limit at sea level (s), 0 = none
    pub ab_time_limit_s: f64,
}

impl TurbojetSpec {
    /// General Electric J85-GE-5A: 2,680 lbf dry / 3,850 lbf afterburning, T-38A Talon.
    pub fn j85_ge_5a() -> Self {
        TurbojetSpec {
            name: "J85-GE-5A".into(),
            manufacturer: "General Electric".into(),
            application: "Northrop T-38A Talon (two engines)".into(),
            rating: TjRating { thrust_dry_n: 2_680.0 * LBF, thrust_ab_n: 3_850.0 * LBF, n_100pct_rpm: 16_500.0 },
            geometry: TjGeometry { length_m: 2.76, diameter_m: 0.53, dry_mass_kg: 265.0, compressor_stages: 8, turbine_stages: 2 },
            design_point: TjDesign {
                mass_flow_kg_s: 20.0, // 44 lb/s
                compressor_pressure_ratio: 6.8,
                eta_compressor: 0.80,
                eta_burner: 0.98,
                eta_turbine: 0.87,
                eta_mechanical: 0.99,
                burner_pressure_loss: 0.05,
                cooling_bleed_fraction: 0.05,
                power_extraction_w: 15_000.0,
                fuel_lhv_j_kg: 43.0e6,
                flow_exponent: 1.80, // airflow falls fast with speed on a small axial compressor
                pr_exponent: 1.85,
                efficiency_falloff: 0.45,
                ab_efficiency: 0.90,
                ab_pressure_loss_dry: 0.03,
                ab_pressure_loss_lit: 0.08,
                ab_t7_max_k: 1_900.0,
                nozzle_cd: 0.97,
            },
            control: TjControl {
                n_idle_pct: 48.0,
                n_idle_per_km: 1.0,
                lever_exponent: 1.3,
                governor_gain_kg_s_per_pct: 0.0025,
                // Steady Wf/Ps3 runs from ~0.65 (kg/h)/kPa at idle to ~1.7 at military
                accel_wf_p3_vs_nc: vec![[45.0, 1.30], [60.0, 1.50], [75.0, 1.95], [90.0, 2.45], [100.0, 2.70], [105.0, 2.70]],
                // The floor must stay below the steady fuel with the nozzle open
                // (throttle chop from military: the nozzle opens as the lever retards)
                decel_wf_p3_vs_nc: vec![[45.0, 0.35], [70.0, 0.42], [100.0, 0.55], [105.0, 0.55]],
                min_fuel_kg_s: 0.010,
                fuel_lag_s: 0.15,
                nozzle_idle_frac: 1.35,
                nozzle_ab_max_frac: 1.55,
                nozzle_rate_per_s: 0.6,
                ab_min_n_pct: 95.0,
                ab_light_delay_s: 1.5,
                ab_ramp_s: 2.0,
                inertia_kg_m2: 0.16,
                surge_margin_pct: 16.0,
            },
            start: TjStart {
                starter_max_n_pct: 22.0,
                starter_tau_s: 6.0,
                min_light_n_pct: 12.0,
                light_off_delay_s: 2.0,
                starter_cutout_n_pct: 45.0,
                max_light_altitude_m: 7_600.0,
                windmill_n_per_mach: 25.0,
                start_fuel_schedule: vec![[12.0, 0.010], [20.0, 0.014], [30.0, 0.020], [40.0, 0.028], [48.0, 0.036], [50.0, 0.037]],
                start_accel_vs_n: vec![[12.0, 1.6], [18.0, 2.6], [25.0, 3.3], [35.0, 4.6], [45.0, 6.4], [48.0, 7.0], [52.0, 6.6]],
            },
            oil: TjOil { capacity_qt: 5.5, full_qt: 5.0, press_gain_psi_per_pct: 0.55, press_offset_psi: -8.0, temp_rise_idle_k: 40.0, temp_rise_mil_k: 75.0, temp_tau_s: 150.0, starvation_qt: 1.5, seizure_time_s: 60.0 },
            limits: TjLimits {
                n_max_pct: 100.5,
                egt_max_c: 690.0,
                egt_transient_c: 735.0,
                egt_start_c: 760.0,
                oil_press_min_psi: 25.0,
                oil_press_min_idle_psi: 5.0,
                oil_press_max_psi: 50.0,
                oil_temp_max_c: 135.0,
                ab_time_limit_s: 0.0,
            },
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("spec serialises")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TjControls {
    /// Throttle: 0 idle .. 1 military .. 1.3 maximum afterburner
    pub throttle: f64,
    /// Throttle out of CUTOFF (fuel on)
    pub fuel: bool,
    pub starter: bool,
    pub anti_ice: bool,
}

impl Default for TjControls {
    fn default() -> Self {
        TjControls { throttle: 0.0, fuel: false, starter: false, anti_ice: false }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TjEnvironment {
    pub altitude_m: f64,
    pub mach: f64,
    pub delta_isa_k: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TjFaults {
    pub starter_inop: bool,
    pub starter_weak: bool,
    pub ignition_fail: bool,
    pub start_fuel_factor: f64,
    pub fuel_pump_fail: bool,
    pub oil_leak_qt_per_min: f64,
    pub fire: bool,
    pub compressor_damage_pct: f64,
    pub fod: bool,
    pub egt_probe_fail: bool,
    /// Nozzle actuator frozen at its current position
    pub nozzle_stuck: bool,
    /// Afterburner will not light (igniter / fuel manifold)
    pub ab_fail: bool,
    pub trigger_surge: bool,
    pub trigger_flameout: bool,
    pub fire_bottle: bool,
}

impl TjFaults {
    pub fn none() -> Self {
        TjFaults { start_fuel_factor: 1.0, ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TjMode {
    Off,
    Motoring,
    Starting,
    Running,
    Shutdown,
}

/// Cycle result at one operating point.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TjCycle {
    pub w2: f64,
    pub wf: f64,
    pub wf_ab: f64,
    pub pr: f64,
    pub tt3: f64,
    pub pt3: f64,
    pub t4: f64,
    pub pt4: f64,
    pub tt5: f64,
    pub pt5: f64,
    pub tt7: f64,
    pub pt7: f64,
    pub turbine_pr: f64,
    pub compressor_power_w: f64,
    pub turbine_power_w: f64,
    pub v9: f64,
    pub npr: f64,
    pub choked: bool,
    pub a8_required_m2: f64,
    pub gross_thrust_n: f64,
    pub ram_drag_n: f64,
    pub net_thrust_n: f64,
    pub ps3_kpa: f64,
    pub tsfc: f64,
    pub thermal_efficiency: f64,
    pub propulsive_efficiency: f64,
}

fn compress(tt: f64, pr: f64, eta: f64) -> f64 {
    tt * (1.0 + (pr.powf(0.4 / 1.4) - 1.0) / eta)
}

fn turbine_pr(tt_in: f64, dt: f64, eta: f64) -> f64 {
    let k = (GAMMA_GAS - 1.0) / GAMMA_GAS;
    (1.0 - (dt / tt_in) / eta).max(0.02).powf(-1.0 / k)
}

/// Burner balance for a stream of heat capacity `cp_in` (air for the main
/// combustor, combustion gas for the afterburner); see [`burner_fuel`].
fn fuel_for_t(w: f64, cp_in: f64, t_in: f64, t_out: f64, eta: f64, lhv: f64) -> f64 {
    burner_fuel(w, cp_in, t_in, t_out, eta, lhv)
}

fn t_for_fuel(w: f64, cp_in: f64, t_in: f64, wf: f64, eta: f64, lhv: f64) -> f64 {
    burner_t_out(w, cp_in, t_in, wf, eta, lhv, 2_400.0)
}

/// Cycle at corrected speed `nc` (fraction), turbine inlet temperature `t4`,
/// afterburner fuel `wf_ab` and nozzle throat area `a8`.
pub fn tj_cycle(spec: &TurbojetSpec, inlet: &Inlet, nc: f64, t4: f64, combustion: bool, wf_ab: f64, a8: f64, bleed: f64) -> TjCycle {
    let d = &spec.design_point;
    let n = nc.max(0.0);
    let corr = inlet.delta / inlet.theta.sqrt();
    let w2 = (d.mass_flow_kg_s * corr * n.powf(d.flow_exponent)).max(1e-3);
    let fo = |eta: f64| eta * (1.0 - d.efficiency_falloff * (1.0 - n.min(1.0)).powi(2));
    // Unpowered (windmilling) the compressor is a loss, not a pump
    let pr = if combustion { 1.0 + (d.compressor_pressure_ratio - 1.0) * n.powf(d.pr_exponent) } else { 1.0 - WINDMILL_PT_LOSS };
    let tt3 = if combustion { compress(inlet.tt2_k, pr, fo(d.eta_compressor)) } else { inlet.tt2_k };
    let pt3 = inlet.pt2_pa * pr;
    let w_b = w2 * (1.0 - d.cooling_bleed_fraction - bleed);
    let t4 = if combustion { t4.max(tt3) } else { tt3 };
    let wf = if combustion { fuel_for_t(w_b, CP_AIR, tt3, t4, d.eta_burner, d.fuel_lhv_j_kg) } else { 0.0 };
    let pt4 = pt3 * (1.0 - d.burner_pressure_loss);
    let w4 = w_b + wf;
    let comp_power = w2 * CP_AIR * (tt3 - inlet.tt2_k) + d.power_extraction_w;
    let turb_power = if combustion { comp_power / d.eta_mechanical } else { 0.0 };
    let dt_t = turb_power / (w4 * CP_GAS);
    let tt5_u = t4 - dt_t;
    let t_pr = if combustion { turbine_pr(t4, dt_t, fo(d.eta_turbine)) } else { 1.0 / (1.0 - WINDMILL_PT_LOSS) };
    let pt5 = pt4 / t_pr;
    let w5 = w2 * (1.0 - bleed) + wf;
    let tt5 = (w4 * CP_GAS * tt5_u + d.cooling_bleed_fraction * w2 * CP_AIR * tt3) / (w5 * CP_GAS);
    // Afterburner
    let wf_ab = if combustion { wf_ab.max(0.0) } else { 0.0 };
    let lit = wf_ab > 1e-6;
    let tt7 = if lit { t_for_fuel(w5, CP_GAS, tt5, wf_ab, d.ab_efficiency, d.fuel_lhv_j_kg) } else { tt5 };
    let pt7 = pt5 * (1.0 - if lit { d.ab_pressure_loss_lit } else { d.ab_pressure_loss_dry });
    let w9 = w5 + wf_ab;
    let p0 = inlet.ambient.pressure_pa;
    let ex = nozzle(w9, tt7, pt7, p0, GAMMA_GAS, CP_GAS);
    let a8_required = ex.required_area_m2 / d.nozzle_cd;
    // Thrust: momentum plus the pressure term on the throat passing this flow
    // when choked. (The actual area `a8` sets the operating line through the
    // T4 solve; thrust at a given state does not depend on it.)
    let _ = a8;
    let gross = w9 * ex.exit_velocity_m_s + if ex.choked { (ex.exit_static_pressure_pa - p0) * ex.required_area_m2 } else { 0.0 };
    let ram = w2 * inlet.true_airspeed_m_s;
    let net = gross - ram;
    let wf_total = wf + wf_ab;
    let v0 = inlet.true_airspeed_m_s;
    let ke = 0.5 * w9 * ex.exit_velocity_m_s.powi(2) - 0.5 * w2 * v0 * v0;
    TjCycle {
        w2,
        wf,
        wf_ab,
        pr,
        tt3,
        pt3,
        t4,
        pt4,
        tt5,
        pt5,
        tt7,
        pt7,
        turbine_pr: t_pr,
        compressor_power_w: comp_power,
        turbine_power_w: turb_power,
        v9: ex.exit_velocity_m_s,
        npr: ex.npr,
        choked: ex.choked,
        a8_required_m2: a8_required,
        gross_thrust_n: gross,
        ram_drag_n: ram,
        net_thrust_n: net,
        ps3_kpa: pt3 * 0.97 / 1000.0,
        tsfc: if net > 1.0 { wf_total / net } else { 0.0 },
        thermal_efficiency: if wf_total > 0.0 { (ke / (wf_total * d.fuel_lhv_j_kg)).clamp(0.0, 1.0) } else { 0.0 },
        propulsive_efficiency: if ke > 0.0 && v0 > 0.1 { (net * v0 / ke).clamp(0.0, 1.0) } else { 0.0 },
    }
}

/// T4 at which the exhaust flow fits the nozzle throat area `a8`.
pub fn tj_solve_t4(spec: &TurbojetSpec, inlet: &Inlet, nc: f64, wf_ab: f64, a8: f64, bleed: f64) -> f64 {
    let probe = tj_cycle(spec, inlet, nc, inlet.tt2_k, false, 0.0, a8, bleed);
    let f = |t4: f64| tj_cycle(spec, inlet, nc, t4, true, wf_ab, a8, bleed).a8_required_m2 - a8;
    let (mut lo, mut hi) = (probe.tt3 + 20.0, 2_300.0);
    // Required area falls as T4 rises (higher turbine PR, lower P7 is more than
    // offset ... in practice the flow function makes A8_required decrease with T4)
    if f(lo) <= 0.0 {
        return lo;
    }
    if f(hi) >= 0.0 {
        return hi;
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 { lo = mid } else { hi = mid }
        if hi - lo < 0.01 { break }
    }
    0.5 * (lo + hi)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjSizing {
    /// Nozzle throat area at military power (m^2)
    pub a8_mil_m2: f64,
    pub t4_mil_k: f64,
    /// Afterburner fuel flow at maximum (kg/s) and the nozzle area it needs
    pub wf_ab_max_kg_s: f64,
    pub a8_ab_max_m2: f64,
    pub military: TjCycle,
    pub max_ab: TjCycle,
}

/// Size the nozzle so that 100 % speed gives the rated dry thrust (SL static
/// ISA), then the afterburner fuel so that full reheat gives the rated
/// afterburning thrust with the nozzle open to pass the reheated flow at the
/// same turbine operating point.
pub fn tj_size(spec: &TurbojetSpec) -> TjSizing {
    let inlet = inlet_conditions(isa(0.0, 0.0), 0.0);
    let probe = tj_cycle(spec, &inlet, 1.0, inlet.tt2_k, false, 0.0, 0.1, 0.0);
    let (mut lo, mut hi) = (probe.tt3 + 50.0, 2_000.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if tj_cycle(spec, &inlet, 1.0, mid, true, 0.0, 0.1, 0.0).net_thrust_n < spec.rating.thrust_dry_n { lo = mid } else { hi = mid }
        if hi - lo < 0.005 { break }
    }
    let t4 = 0.5 * (lo + hi);
    let mil = tj_cycle(spec, &inlet, 1.0, t4, true, 0.0, 0.1, 0.0);
    let a8_mil = mil.a8_required_m2;
    let military = tj_cycle(spec, &inlet, 1.0, t4, true, 0.0, a8_mil, 0.0);
    // Afterburner: the same turbine point, reheat fuel for the AB rating
    let (mut lo, mut hi) = (0.0, 2.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        let r = tj_cycle(spec, &inlet, 1.0, t4, true, mid, a8_mil, 0.0);
        let r = tj_cycle(spec, &inlet, 1.0, t4, true, mid, r.a8_required_m2, 0.0);
        if r.net_thrust_n < spec.rating.thrust_ab_n { lo = mid } else { hi = mid }
        if hi - lo < 1e-5 { break }
    }
    let wf_ab = 0.5 * (lo + hi);
    let r = tj_cycle(spec, &inlet, 1.0, t4, true, wf_ab, a8_mil, 0.0);
    let a8_ab = r.a8_required_m2;
    let max_ab = tj_cycle(spec, &inlet, 1.0, t4, true, wf_ab, a8_ab, 0.0);
    TjSizing { a8_mil_m2: a8_mil, t4_mil_k: t4, wf_ab_max_kg_s: wf_ab, a8_ab_max_m2: a8_ab, military, max_ab }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TjState {
    pub kind: String,
    pub time_s: f64,
    pub mode: TjMode,
    pub controls: TjControls,
    pub environment: TjEnvironment,
    pub faults: TjFaults,
    pub inlet: Inlet,
    pub n_pct: f64,
    pub n_rpm: f64,
    pub n_corrected_pct: f64,
    pub n_command_pct: f64,
    pub n_idle_pct: f64,
    pub egt_c: f64,
    pub egt_valid: bool,
    pub egt_gas_c: f64,
    pub t4_k: f64,
    pub t4_steady_k: f64,
    pub thrust_n: f64,
    pub thrust_lbf: f64,
    pub thrust_pct: f64,
    pub tsfc_lb_lbf_h: f64,
    pub fuel_flow_kg_s: f64,
    pub fuel_flow_pph: f64,
    pub fuel_flow_steady_kg_s: f64,
    pub fuel_command_kg_s: f64,
    pub fuel_accel_limit_kg_s: f64,
    pub fuel_decel_limit_kg_s: f64,
    pub ab_fuel_flow_kg_s: f64,
    pub ab_fuel_flow_pph: f64,
    pub wf_p3_ratio: f64,
    pub ps3_kpa: f64,
    pub fuel_used_kg: f64,
    /// Nozzle position: 0 % = closed (military), 100 % = fully open
    pub nozzle_pct: f64,
    pub nozzle_area_m2: f64,
    pub nozzle_command_pct: f64,
    pub ab_lit: bool,
    pub ab_time_s: f64,
    pub oil_pressure_psi: f64,
    pub oil_temperature_c: f64,
    pub oil_quantity_qt: f64,
    pub vib: f64,
    pub starter_engaged: bool,
    pub lit: bool,
    pub surge_margin_pct: f64,
    pub surge: bool,
    pub surge_count: u32,
    pub flameout_count: u32,
    pub fire_warning: bool,
    pub seized: bool,
    pub windmill_n_pct: f64,
    pub cycle: TjCycle,
    pub run_time_s: f64,
    pub warnings: Vec<String>,
    pub logger_rows: usize,
    pub logger_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbojetEngine {
    pub spec: TurbojetSpec,
    pub sizing: TjSizing,
    pub controls: TjControls,
    pub environment: TjEnvironment,
    pub faults: TjFaults,
    pub logger: DataLogger,
    time_s: f64,
    mode: TjMode,
    n: f64,
    n_cmd: f64,
    n_idle: f64,
    lit: bool,
    fuel_on_since: Option<f64>,
    starter_engaged: bool,
    wf: f64,
    wf_cmd: f64,
    wf_max: f64,
    wf_min: f64,
    wf_ab: f64,
    ab_lit: bool,
    ab_request_since: Option<f64>,
    ab_time_s: f64,
    a8: f64,
    a8_cmd: f64,
    t4: f64,
    t4_ss: f64,
    ps3_kpa: f64,
    egt_gas_k: f64,
    egt_sensed_k: f64,
    ff_sensed: f64,
    ff_steady: f64,
    fuel_used_kg: f64,
    oil_temp_k: f64,
    oil_press_psi: f64,
    oil_qty_qt: f64,
    oil_starved_s: f64,
    seized: bool,
    surge_timer: f64,
    surge_recovery_s: f64,
    surge_count: u32,
    flameout_count: u32,
    sm: f64,
    hung_timer: f64,
    bottle_msg_timer: f64,
    run_time_s: f64,
    seed: u64,
    idle_fuel_kg_s: f64,
    last_cycle: TjCycle,
    warnings: Vec<String>,
    max_substep_s: f64,
}

pub const TJ_LOG_COLUMNS: &[&str] = &[
    "time_s", "mode", "throttle", "fuel", "starter", "anti_ice",
    "altitude_m", "mach", "delta_isa_k", "tt2_k", "pt2_pa",
    "n_pct", "n_corrected_pct", "n_command_pct", "egt_c", "egt_gas_c", "t4_k", "t4_steady_k",
    "thrust_n", "thrust_lbf", "thrust_pct", "tsfc_lb_lbf_h",
    "fuel_flow_kg_s", "fuel_flow_pph", "fuel_command_kg_s", "fuel_accel_limit_kg_s", "fuel_decel_limit_kg_s", "ab_fuel_flow_kg_s", "wf_p3_ratio", "ps3_kpa", "fuel_used_kg",
    "nozzle_pct", "nozzle_area_m2", "ab_lit", "oil_pressure_psi", "oil_temperature_c", "oil_quantity_qt", "vib",
    "surge_margin_pct", "surge", "lit", "fire",
    "w2_kg_s", "compressor_pr", "tt3_k", "pt3_pa", "tt5_k", "pt5_pa", "tt7_k", "npr", "v9_m_s", "thermal_eff", "propulsive_eff",
];

impl TurbojetEngine {
    pub fn new(spec: TurbojetSpec) -> Self {
        let sizing = tj_size(&spec);
        let env = TjEnvironment::default();
        let inl = inlet_conditions(isa(env.altitude_m, env.delta_isa_k), env.mach);
        let a8_idle = sizing.a8_mil_m2 * spec.control.nozzle_idle_frac;
        let cold = tj_cycle(&spec, &inl, 0.0, inl.tt2_k, false, 0.0, a8_idle, 0.0);
        let idle_fuel_kg_s = {
            let nc = spec.control.n_idle_pct / 100.0;
            let t4 = tj_solve_t4(&spec, &inl, nc, 0.0, a8_idle, 0.0);
            tj_cycle(&spec, &inl, nc, t4, true, 0.0, a8_idle, 0.0).wf
        };
        let columns = TJ_LOG_COLUMNS.iter().map(|s| s.to_string()).collect();
        let full = spec.oil.full_qt;
        let idle = spec.control.n_idle_pct;
        TurbojetEngine {
            spec,
            sizing,
            controls: TjControls::default(),
            environment: env,
            faults: TjFaults::none(),
            logger: DataLogger::new(10.0, columns),
            time_s: 0.0,
            mode: TjMode::Off,
            n: 0.0,
            n_cmd: 0.0,
            n_idle: idle,
            lit: false,
            fuel_on_since: None,
            starter_engaged: false,
            wf: 0.0,
            wf_cmd: 0.0,
            wf_max: 0.0,
            wf_min: 0.0,
            wf_ab: 0.0,
            ab_lit: false,
            ab_request_since: None,
            ab_time_s: 0.0,
            a8: a8_idle,
            a8_cmd: a8_idle,
            t4: inl.tt2_k,
            t4_ss: inl.tt2_k,
            ps3_kpa: inl.pt2_pa / 1000.0,
            egt_gas_k: inl.tt2_k,
            egt_sensed_k: inl.tt2_k,
            ff_sensed: 0.0,
            ff_steady: 0.0,
            fuel_used_kg: 0.0,
            oil_temp_k: inl.tt2_k,
            oil_press_psi: 0.0,
            oil_qty_qt: full,
            oil_starved_s: 0.0,
            seized: false,
            surge_timer: 0.0,
            surge_recovery_s: 0.0,
            surge_count: 0,
            flameout_count: 0,
            sm: 0.0,
            hung_timer: 0.0,
            bottle_msg_timer: 0.0,
            run_time_s: 0.0,
            seed: 0x5DEECE66D,
            idle_fuel_kg_s,
            last_cycle: cold,
            warnings: Vec::new(),
            max_substep_s: 0.01,
        }
    }

    pub fn reset(&mut self) {
        let spec = self.spec.clone();
        let (c, e, f, rate) = (self.controls, self.environment, self.faults, self.logger.rate_hz);
        *self = TurbojetEngine::new(spec);
        self.controls = c;
        self.environment = e;
        self.faults = f;
        self.logger.set_rate(rate);
    }

    pub fn time(&self) -> f64 {
        self.time_s
    }

    fn inlet(&self) -> Inlet {
        inlet_conditions(isa(self.environment.altitude_m, self.environment.delta_isa_k), self.environment.mach)
    }

    fn bleed(&self) -> f64 {
        if self.controls.anti_ice { 0.02 } else { 0.0 }
    }

    fn idle_corrected(&self) -> f64 {
        self.spec.control.n_idle_pct + self.spec.control.n_idle_per_km * (self.environment.altitude_m / 1000.0).max(0.0)
    }

    fn n_command_corrected(&self, inl: &Inlet) -> f64 {
        let c = &self.spec.control;
        let st = inl.theta.sqrt();
        let idle_c = self.n_idle / st;
        let max_c = (100.0f64).min(self.spec.limits.n_max_pct / st);
        let x = self.controls.throttle.clamp(0.0, 1.0);
        idle_c + (max_c - idle_c) * x.powf(c.lever_exponent)
    }

    /// Nozzle area commanded by the throttle and the afterburner state
    fn nozzle_command(&self) -> f64 {
        let c = &self.spec.control;
        let z = &self.sizing;
        let t = self.controls.throttle;
        if self.ab_lit {
            let y = ((t - 1.0) / 0.3).clamp(0.0, 1.0);
            let open = 1.0 + (z.a8_ab_max_m2 / z.a8_mil_m2 - 1.0) * y;
            z.a8_mil_m2 * open.min(c.nozzle_ab_max_frac)
        } else {
            let x = t.clamp(0.0, 1.0);
            z.a8_mil_m2 * (c.nozzle_idle_frac + (1.0 - c.nozzle_idle_frac) * x)
        }
    }

    fn oil_temp_target(&self, inl: &Inlet) -> f64 {
        let o = &self.spec.oil;
        if matches!(self.mode, TjMode::Running | TjMode::Starting) {
            let load = ((self.n - self.spec.control.n_idle_pct) / (100.0 - self.spec.control.n_idle_pct)).clamp(0.0, 1.0);
            inl.tt2_k + o.temp_rise_idle_k + (o.temp_rise_mil_k - o.temp_rise_idle_k) * load
        } else {
            inl.tt2_k
        }
    }

    fn noise(&mut self) -> f64 {
        let mut x = self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64) - 0.5
    }

    /// Jump to a stabilised running condition at the given throttle (0..1.3).
    pub fn set_running(&mut self, throttle: f64) {
        self.controls.throttle = throttle.clamp(0.0, 1.3);
        self.controls.fuel = true;
        self.controls.starter = false;
        self.seized = false;
        let inl = self.inlet();
        let st = inl.theta.sqrt();
        self.n_idle = self.idle_corrected() * st;
        self.n = self.n_command_corrected(&inl) * st;
        self.n_cmd = self.n;
        self.mode = TjMode::Running;
        self.lit = true;
        self.ab_lit = false;
        self.wf_ab = 0.0;
        self.a8 = self.nozzle_command();
        self.a8_cmd = self.a8;
        let nc = self.n / st / 100.0;
        let t4 = tj_solve_t4(&self.spec, &inl, nc, 0.0, self.a8, self.bleed());
        let r = tj_cycle(&self.spec, &inl, nc, t4, true, 0.0, self.a8, self.bleed());
        self.t4 = t4;
        self.t4_ss = t4;
        self.wf = r.wf;
        self.wf_cmd = r.wf;
        self.ff_sensed = r.wf;
        self.ff_steady = r.wf;
        self.egt_gas_k = r.tt5;
        self.egt_sensed_k = r.tt5;
        self.oil_temp_k = self.oil_temp_target(&inl);
        self.last_cycle = r;
        // Let the afterburner light and the nozzle settle if requested
        for _ in 0..600 {
            self.substep(0.01);
        }
        self.time_s = 0.0;
        self.run_time_s = 0.0;
        self.ab_time_s = 0.0;
        self.fuel_used_kg = 0.0;
        self.logger.clear();
    }

    pub fn step(&mut self, dt: f64) {
        if dt <= 0.0 {
            return;
        }
        let n = (dt / self.max_substep_s).ceil().max(1.0) as usize;
        let h = dt / n as f64;
        for _ in 0..n {
            self.substep(h);
        }
    }

    fn substep(&mut self, h: f64) {
        let spec = self.spec.clone();
        let c = self.controls;
        let f = self.faults;
        let ctl = &spec.control;
        let sp = &spec.start;
        let inl = self.inlet();
        let st = inl.theta.sqrt();
        self.warnings.clear();
        self.faults.trigger_surge = false;
        self.faults.trigger_flameout = false;
        self.faults.fire_bottle = false;
        self.n_idle = self.idle_corrected() * st;

        let n_wm = (sp.windmill_n_per_mach * self.environment.mach).clamp(0.0, 30.0);
        let starter_ok = c.starter && !f.starter_inop && !self.seized;
        let starter_max = if f.starter_weak { 11.0 } else { sp.starter_max_n_pct };
        let fuel_available = c.fuel && !f.fuel_pump_fail;
        let ignition_on = starter_ok || self.mode == TjMode::Running || self.mode == TjMode::Starting;
        let light_possible = self.n >= sp.min_light_n_pct && self.environment.altitude_m <= sp.max_light_altitude_m && !f.ignition_fail && ignition_on;

        // ---- Mode transitions ----------------------------------------------
        match self.mode {
            TjMode::Off => {
                if starter_ok { self.mode = TjMode::Motoring }
                else if fuel_available && self.n >= sp.min_light_n_pct { self.mode = TjMode::Starting; self.fuel_on_since = Some(self.time_s) }
            }
            TjMode::Motoring => {
                if fuel_available && self.n >= sp.min_light_n_pct { self.mode = TjMode::Starting; self.fuel_on_since = Some(self.time_s) }
                else if !starter_ok { self.mode = TjMode::Off }
            }
            TjMode::Starting => {
                if !fuel_available || self.seized { self.mode = TjMode::Shutdown; self.lit = false; self.fuel_on_since = None }
                else if self.lit && self.n >= self.n_idle - 0.3 { self.mode = TjMode::Running; self.n_cmd = self.n; self.hung_timer = 0.0 }
            }
            TjMode::Running => {
                if !fuel_available || self.seized { self.mode = TjMode::Shutdown; self.lit = false; self.fuel_on_since = None; self.ab_lit = false }
                else if !self.lit { self.mode = TjMode::Starting; self.fuel_on_since = Some(self.time_s); self.ab_lit = false }
                else if self.n < self.n_idle - 8.0 { self.mode = TjMode::Starting; self.fuel_on_since = Some(self.time_s - sp.light_off_delay_s); self.surge_timer = 0.0; self.ab_lit = false }
            }
            TjMode::Shutdown => {
                if fuel_available && self.n >= sp.min_light_n_pct && !self.seized { self.mode = TjMode::Starting; self.fuel_on_since = Some(self.time_s) }
                else if starter_ok && self.n < starter_max { self.mode = TjMode::Motoring }
                else if self.n < 1.0 { self.mode = TjMode::Off }
            }
        }
        self.starter_engaged = starter_ok && self.n < sp.starter_cutout_n_pct && self.mode != TjMode::Running;
        let starter_rate0 = starter_max / sp.starter_tau_s;
        let starter_assist = if self.starter_engaged { (starter_rate0 * (1.0 - self.n / (starter_max * 2.8))).max(0.0) } else { 0.0 };
        let drag_rate = |n: f64| (n - n_wm).max(0.0) / 9.0;

        // ---- Nozzle actuator -------------------------------------------------
        self.a8_cmd = self.nozzle_command();
        if !f.nozzle_stuck {
            let rate = ctl.nozzle_rate_per_s * self.sizing.a8_mil_m2;
            self.a8 += (self.a8_cmd - self.a8).clamp(-rate * h, rate * h);
        }
        let a8 = self.a8;
        let bleed = self.bleed();

        // ---- Spool dynamics ----------------------------------------------------
        let combustion;
        let mut wf_dyn = 0.0;
        let nc = self.n / st / 100.0;
        match self.mode {
            TjMode::Off | TjMode::Motoring | TjMode::Shutdown => {
                combustion = false;
                if self.seized { self.n += (0.0 - self.n) / 3.0 * h }
                else if self.starter_engaged { self.n += (starter_assist - drag_rate(self.n)) * h }
                else { self.n += (n_wm - self.n) / (if self.mode == TjMode::Shutdown { 14.0 } else { 9.0 }) * h }
                self.t4 = inl.tt2_k;
                self.t4_ss = inl.tt2_k;
                self.wf = 0.0;
                self.wf_cmd = 0.0;
                self.wf_max = 0.0;
                self.wf_min = 0.0;
                self.wf_ab = 0.0;
                self.ab_lit = false;
                self.ab_request_since = None;
            }
            TjMode::Starting => {
                if let Some(t0) = self.fuel_on_since {
                    if !self.lit && self.time_s - t0 >= sp.light_off_delay_s && light_possible { self.lit = true }
                    if !self.lit && self.time_s - t0 > sp.light_off_delay_s + 8.0 {
                        if f.ignition_fail || !ignition_on { self.warnings.push("NO LIGHT: ignition inoperative (wet start, fuel accumulating)".into()) }
                        else if self.n < sp.min_light_n_pct { self.warnings.push(format!("NO LIGHT: N {:.0}% below {:.0}%", self.n, sp.min_light_n_pct)) }
                    }
                }
                combustion = self.lit;
                let sched_idle = interp(&sp.start_fuel_schedule, ctl.n_idle_pct).max(1e-6);
                let scale = self.idle_fuel_kg_s * inl.delta / sched_idle * f.start_fuel_factor.max(0.1);
                wf_dyn = interp(&sp.start_fuel_schedule, self.n / st) * scale;
                self.wf = wf_dyn;
                self.wf_cmd = wf_dyn;
                let mut n_dot = starter_assist - drag_rate(self.n);
                if self.lit { n_dot += interp(&sp.start_accel_vs_n, self.n / st) * (0.4 + 0.6 * f.start_fuel_factor).max(0.2) }
                self.n += n_dot * h;
                if self.lit && self.n < sp.min_light_n_pct - 3.0 { self.lit = false; self.fuel_on_since = Some(self.time_s) }
                if self.lit && !self.starter_engaged && self.n < self.n_idle - 3.0 {
                    if n_dot < 0.15 { self.hung_timer += h } else { self.hung_timer = 0.0 }
                    if self.hung_timer > 6.0 { self.warnings.push(format!("HUNG START: N stalled at {:.0}%", self.n)) }
                }
                self.wf_ab = 0.0;
            }
            TjMode::Running => {
                combustion = true;
                let nc_cmd = self.n_command_corrected(&inl);
                self.n_cmd = nc_cmd * st;
                // Afterburner request, light and fuel ramp
                let ab_wanted = c.throttle > 1.0 + 1e-6 && !f.ab_fail && !f.fuel_pump_fail;
                if ab_wanted {
                    if self.ab_request_since.is_none() { self.ab_request_since = Some(self.time_s) }
                    let armed = self.n >= ctl.ab_min_n_pct * st.min(1.0) && self.environment.altitude_m <= sp.max_light_altitude_m + 6_000.0;
                    if !self.ab_lit && armed && self.time_s - self.ab_request_since.unwrap_or(self.time_s) >= ctl.ab_light_delay_s { self.ab_lit = true }
                } else {
                    self.ab_request_since = None;
                    self.ab_lit = false;
                }
                let ab_target = if self.ab_lit { self.sizing.wf_ab_max_kg_s * ((c.throttle - 1.0) / 0.3).clamp(0.05, 1.0) * inl.delta } else { 0.0 };
                let ab_rate = self.sizing.wf_ab_max_kg_s / ctl.ab_ramp_s;
                self.wf_ab += (ab_target - self.wf_ab).clamp(-ab_rate * 3.0 * h, ab_rate * h);
                if self.ab_lit { self.ab_time_s += h } else { self.ab_time_s = 0.0 }
                // Steady point at the actual nozzle area and reheat. A nozzle too
                // closed for the reheated flow (stuck while the AB lights) pushes
                // the required T4 up; it is capped at what the turbine can take and
                // the operating line rises toward stall instead.
                let t4_ss = tj_solve_t4(&spec, &inl, nc, self.wf_ab, a8, bleed).min(self.sizing.t4_mil_k * 1.15);
                let ss = tj_cycle(&spec, &inl, nc, t4_ss, true, self.wf_ab, a8, bleed);
                self.t4_ss = t4_ss;
                self.ff_steady = ss.wf;
                self.ps3_kpa = ss.ps3_kpa;
                let demand = ss.wf + ctl.governor_gain_kg_s_per_pct * (self.n_cmd - self.n);
                let nc_pct = self.n / st;
                self.wf_max = interp(&ctl.accel_wf_p3_vs_nc, nc_pct) * self.ps3_kpa / 3600.0;
                self.wf_min = (interp(&ctl.decel_wf_p3_vs_nc, nc_pct) * self.ps3_kpa / 3600.0).max((ctl.min_fuel_kg_s * inl.delta).min(0.9 * ss.wf));
                if self.surge_recovery_s > 0.0 {
                    self.surge_recovery_s -= h;
                    let fr = 1.0 - self.surge_recovery_s / 4.0;
                    self.wf_max = ss.wf + (self.wf_max - ss.wf) * fr.clamp(0.0, 1.0);
                }
                self.wf_cmd = demand;
                let mut target = demand.clamp(self.wf_min, self.wf_max.max(self.wf_min));
                if self.surge_timer > 0.0 { target = self.wf_min }
                self.wf += (target - self.wf) / ctl.fuel_lag_s * h;
                wf_dyn = self.wf;
                let d = &spec.design_point;
                let w_b = ss.w2 * (1.0 - d.cooling_bleed_fraction - bleed);
                self.t4 = t_for_fuel(w_b, CP_AIR, ss.tt3, self.wf, d.eta_burner, d.fuel_lhv_j_kg);
                let ratio = (self.t4 / self.t4_ss).max(0.2);
                let p_excess = ss.turbine_power_w * (ratio - 1.0);
                let w = (self.n / 100.0 * spec.rating.n_100pct_rpm * TAU / 60.0).max(50.0);
                let mut a = p_excess / (ctl.inertia_kg_m2 * w);
                if self.surge_timer > 0.0 { a -= 0.06 * w }
                self.n += a / (spec.rating.n_100pct_rpm * TAU / 60.0) * 100.0 * h;
                self.n = self.n.max(n_wm);
                let sm_avail = ctl.surge_margin_pct * (0.55 + 0.45 * nc.min(1.0)) - f.compressor_damage_pct - if f.fod { 4.0 } else { 0.0 };
                // Over-fuelling raises the operating line; a nozzle closed beyond the
                // steady schedule (stuck while the AB lights) does the same.
                let a8_ratio = (ss.a8_required_m2 / a8).max(0.5);
                self.sm = ((1.0 + sm_avail / 100.0) / (ratio.powf(0.3) * a8_ratio) - 1.0) * 100.0;
                if self.surge_timer <= 0.0 && (self.sm < 0.0 || f.trigger_surge) { self.surge_timer = 1.5; self.surge_recovery_s = 4.0; self.surge_count += 1 }
                if f.trigger_flameout || self.wf < 0.5 * ctl.min_fuel_kg_s * inl.delta { self.lit = false; self.flameout_count += 1 }
            }
        }
        if self.surge_timer > 0.0 { self.surge_timer -= h; self.warnings.push("COMPRESSOR STALL".into()) }
        self.n = self.n.max(0.0);
        let nc = self.n / st / 100.0;

        // ---- Cycle at the actual point --------------------------------------
        let mut r = if combustion {
            if self.mode == TjMode::Starting {
                let probe = tj_cycle(&spec, &inl, nc, inl.tt2_k, false, 0.0, a8, bleed);
                let w_b = probe.w2 * (1.0 - spec.design_point.cooling_bleed_fraction - bleed);
                self.t4 = t_for_fuel(w_b, CP_AIR, probe.tt3, wf_dyn, spec.design_point.eta_burner, spec.design_point.fuel_lhv_j_kg);
                self.t4_ss = self.t4;
                self.ff_steady = wf_dyn;
            }
            tj_cycle(&spec, &inl, nc, self.t4, true, self.wf_ab, a8, bleed)
        } else {
            self.ff_steady = 0.0;
            tj_cycle(&spec, &inl, nc, inl.tt2_k, false, 0.0, a8, bleed)
        };
        if self.surge_timer > 0.0 { r.net_thrust_n *= 0.3; r.tt5 += 100.0 }
        if f.fod { r.tt5 += 15.0 }
        if self.seized { r.net_thrust_n = 0.0 }

        // ---- EGT probe at the turbine exit ----------------------------------------
        if combustion { self.egt_gas_k += (r.tt5 - self.egt_gas_k) / 0.3 * h }
        else { let tau = 20.0 + 50.0 / (1.0 + self.n / 10.0); self.egt_gas_k += (inl.tt2_k - self.egt_gas_k) / tau * h }
        self.egt_sensed_k += (self.egt_gas_k - self.egt_sensed_k) / 1.2 * h;
        self.ff_sensed += (wf_dyn + self.wf_ab - self.ff_sensed) / 0.4 * h;
        self.fuel_used_kg += (wf_dyn + self.wf_ab) * h;

        // ---- Oil -------------------------------------------------------------------
        let o = &spec.oil;
        self.oil_qty_qt = (self.oil_qty_qt - f.oil_leak_qt_per_min / 60.0 * h).max(0.0);
        let mut oil_target = self.oil_temp_target(&inl);
        if f.fire && matches!(self.mode, TjMode::Running | TjMode::Starting) { oil_target += 60.0 }
        if self.oil_qty_qt < o.starvation_qt { oil_target += 30.0 * (1.0 - self.oil_qty_qt / o.starvation_qt) }
        self.oil_temp_k += (oil_target - self.oil_temp_k) / o.temp_tau_s * h;
        let mut press = o.press_gain_psi_per_pct * self.n + o.press_offset_psi - 0.15 * (self.oil_temp_k - 273.15 - 90.0).max(0.0);
        if self.oil_qty_qt < o.starvation_qt { press *= self.oil_qty_qt / o.starvation_qt }
        self.oil_press_psi = if self.n > 3.0 { press.max(0.0) + 0.3 * self.noise() } else { 0.0 };
        if matches!(self.mode, TjMode::Running | TjMode::Starting) && self.oil_press_psi < spec.limits.oil_press_min_idle_psi { self.oil_starved_s += h } else { self.oil_starved_s = (self.oil_starved_s - h).max(0.0) }
        if self.oil_starved_s > o.seizure_time_s && !self.seized { self.seized = true; self.lit = false }
        if f.fire_bottle { if !c.fuel { self.faults.fire = false; self.bottle_msg_timer = 0.0 } else { self.bottle_msg_timer = 5.0 } }

        if self.mode == TjMode::Running { self.run_time_s += h }
        self.time_s += h;

        // ---- Warnings --------------------------------------------------------------
        let l = &spec.limits;
        let egt_c = self.egt_sensed_k - 273.15;
        if self.faults.fire { self.warnings.push("ENGINE FIRE".into()) }
        if self.seized { self.warnings.push("ENGINE SEIZED (oil starvation)".into()) }
        else if self.oil_starved_s > 5.0 { self.warnings.push(format!("OIL STARVATION {:.0} s", self.oil_starved_s)) }
        if f.egt_probe_fail { self.warnings.push("EGT INDICATION FAILED".into()) }
        if self.mode == TjMode::Starting && egt_c > l.egt_start_c { self.warnings.push(format!("HOT START: EGT {:.0} C exceeds start limit {:.0} C", egt_c, l.egt_start_c)) }
        if self.mode == TjMode::Running {
            if egt_c > l.egt_transient_c { self.warnings.push(format!("EGT {:.0} C exceeds {:.0} C", egt_c, l.egt_transient_c)) }
            else if egt_c > l.egt_max_c && !self.ab_lit { self.warnings.push(format!("EGT {:.0} C above military limit {:.0} C", egt_c, l.egt_max_c)) }
            if self.n > l.n_max_pct { self.warnings.push(format!("OVERSPEED {:.1}% (limit {:.1}%)", self.n, l.n_max_pct)) }
            let min_p = if self.n < 60.0 { l.oil_press_min_idle_psi } else { l.oil_press_min_psi };
            if self.oil_press_psi < min_p { self.warnings.push(format!("LOW OIL PRESSURE {:.0} psi (min {:.0})", self.oil_press_psi, min_p)) }
            if self.oil_temp_k - 273.15 > l.oil_temp_max_c { self.warnings.push(format!("HIGH OIL TEMPERATURE {:.0} C", self.oil_temp_k - 273.15)) }
            if self.sm < 5.0 && self.surge_timer <= 0.0 { self.warnings.push(format!("Stall margin {:.1}%", self.sm)) }
            if f.nozzle_stuck { self.warnings.push(format!("NOZZLE STUCK at {:.0}% open", self.nozzle_pct())) }
            if c.throttle > 1.0 && !self.ab_lit {
                if f.ab_fail { self.warnings.push("AFTERBURNER NO LIGHT".into()) }
                else if self.n < ctl.ab_min_n_pct * st.min(1.0) { self.warnings.push(format!("AB armed: waiting for {:.0}% rpm", ctl.ab_min_n_pct)) }
            }
            if l.ab_time_limit_s > 0.0 && self.ab_time_s > l.ab_time_limit_s { self.warnings.push(format!("AFTERBURNER TIME {:.0} s exceeds limit", self.ab_time_s)) }
        }
        if self.oil_qty_qt < o.starvation_qt { self.warnings.push(format!("LOW OIL QUANTITY {:.1} qt", self.oil_qty_qt)) }
        if self.mode == TjMode::Starting && !self.lit && self.flameout_count > 0 { self.warnings.push("FLAMEOUT: relight in progress".into()) }
        if matches!(self.mode, TjMode::Off | TjMode::Shutdown | TjMode::Motoring) && fuel_available && self.n < sp.min_light_n_pct { self.warnings.push(format!("NO LIGHT: N {:.0}% below {:.0}% (use starter)", self.n, sp.min_light_n_pct)) }
        if self.bottle_msg_timer > 0.0 { self.bottle_msg_timer -= h; self.warnings.push("FIRE BOTTLE DISCHARGED: fire persists, throttle not at CUTOFF".into()) }

        self.last_cycle = r;
        if self.logger.due(self.time_s) {
            let state = self.state_inner(&inl);
            self.logger.maybe_sample(self.time_s, || tj_log_row(&state));
        }
    }

    fn nozzle_pct(&self) -> f64 {
        let z = &self.sizing;
        let open_max = z.a8_mil_m2 * self.spec.control.nozzle_ab_max_frac.max(self.spec.control.nozzle_idle_frac);
        ((self.a8 - z.a8_mil_m2) / (open_max - z.a8_mil_m2) * 100.0).clamp(0.0, 100.0)
    }

    pub fn state(&self) -> TjState {
        let inl = self.inlet();
        self.state_inner(&inl)
    }

    fn state_inner(&self, inl: &Inlet) -> TjState {
        let r = &self.last_cycle;
        let st = inl.theta.sqrt();
        let o = &self.spec.oil;
        let z = &self.sizing;
        let pph = self.ff_sensed * 7936.64;
        let mut seed = self.seed;
        let mut nz = || { seed ^= seed >> 12; seed ^= seed << 25; seed ^= seed >> 27; ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64) - 0.5 };
        let vib = if self.n > 5.0 { (0.3 + 0.7 * (self.n / 100.0).powi(2) + if self.faults.fod { 2.0 } else { 0.0 } + if self.surge_timer > 0.0 { 1.5 } else { 0.0 } + if self.ab_lit { 0.3 } else { 0.0 } + (self.oil_starved_s / o.seizure_time_s).clamp(0.0, 1.0) * 2.0 + 0.1 * nz()).clamp(0.0, 5.0) } else { 0.0 };
        let thrust = r.net_thrust_n;
        let open_max = z.a8_mil_m2 * self.spec.control.nozzle_ab_max_frac.max(self.spec.control.nozzle_idle_frac);
        TjState {
            kind: "turbojet".into(),
            time_s: self.time_s,
            mode: self.mode,
            controls: self.controls,
            environment: self.environment,
            faults: self.faults,
            inlet: *inl,
            n_pct: self.n,
            n_rpm: self.n / 100.0 * self.spec.rating.n_100pct_rpm,
            n_corrected_pct: self.n / st,
            n_command_pct: self.n_cmd,
            n_idle_pct: self.n_idle,
            egt_c: if self.faults.egt_probe_fail { 0.0 } else { self.egt_sensed_k - 273.15 },
            egt_valid: !self.faults.egt_probe_fail,
            egt_gas_c: self.egt_gas_k - 273.15,
            t4_k: self.t4,
            t4_steady_k: self.t4_ss,
            thrust_n: thrust,
            thrust_lbf: thrust / LBF,
            thrust_pct: thrust / self.spec.rating.thrust_dry_n * 100.0,
            tsfc_lb_lbf_h: if thrust > 50.0 { pph / (thrust / LBF) } else { 0.0 },
            fuel_flow_kg_s: self.ff_sensed,
            fuel_flow_pph: pph,
            fuel_flow_steady_kg_s: self.ff_steady,
            fuel_command_kg_s: self.wf_cmd,
            fuel_accel_limit_kg_s: self.wf_max,
            fuel_decel_limit_kg_s: self.wf_min,
            ab_fuel_flow_kg_s: self.wf_ab,
            ab_fuel_flow_pph: self.wf_ab * 7936.64,
            wf_p3_ratio: if self.ps3_kpa > 1.0 { self.wf * 3600.0 / self.ps3_kpa } else { 0.0 },
            ps3_kpa: self.ps3_kpa,
            fuel_used_kg: self.fuel_used_kg,
            nozzle_pct: self.nozzle_pct(),
            nozzle_area_m2: self.a8,
            nozzle_command_pct: ((self.a8_cmd - z.a8_mil_m2) / (open_max - z.a8_mil_m2) * 100.0).clamp(0.0, 100.0),
            ab_lit: self.ab_lit,
            ab_time_s: self.ab_time_s,
            oil_pressure_psi: self.oil_press_psi,
            oil_temperature_c: self.oil_temp_k - 273.15,
            oil_quantity_qt: self.oil_qty_qt,
            vib,
            starter_engaged: self.starter_engaged,
            lit: self.lit,
            surge_margin_pct: self.sm,
            surge: self.surge_timer > 0.0,
            surge_count: self.surge_count,
            flameout_count: self.flameout_count,
            fire_warning: self.faults.fire,
            seized: self.seized,
            windmill_n_pct: (self.spec.start.windmill_n_per_mach * self.environment.mach).clamp(0.0, 30.0),
            cycle: r.clone(),
            run_time_s: self.run_time_s,
            warnings: self.warnings.clone(),
            logger_rows: self.logger.len(),
            logger_enabled: self.logger.enabled,
        }
    }
}

pub fn tj_log_row(s: &TjState) -> Vec<f64> {
    let mode = match s.mode { TjMode::Off => 0.0, TjMode::Motoring => 1.0, TjMode::Starting => 2.0, TjMode::Running => 3.0, TjMode::Shutdown => 4.0 };
    let c = &s.controls;
    let cy = &s.cycle;
    vec![
        s.time_s, mode, c.throttle, c.fuel as u8 as f64, c.starter as u8 as f64, c.anti_ice as u8 as f64,
        s.environment.altitude_m, s.environment.mach, s.environment.delta_isa_k, s.inlet.tt2_k, s.inlet.pt2_pa,
        s.n_pct, s.n_corrected_pct, s.n_command_pct, s.egt_c, s.egt_gas_c, s.t4_k, s.t4_steady_k,
        s.thrust_n, s.thrust_lbf, s.thrust_pct, s.tsfc_lb_lbf_h,
        s.fuel_flow_kg_s, s.fuel_flow_pph, s.fuel_command_kg_s, s.fuel_accel_limit_kg_s, s.fuel_decel_limit_kg_s, s.ab_fuel_flow_kg_s, s.wf_p3_ratio, s.ps3_kpa, s.fuel_used_kg,
        s.nozzle_pct, s.nozzle_area_m2, s.ab_lit as u8 as f64, s.oil_pressure_psi, s.oil_temperature_c, s.oil_quantity_qt, s.vib,
        s.surge_margin_pct, s.surge as u8 as f64, s.lit as u8 as f64, s.fire_warning as u8 as f64,
        cy.w2, cy.pr, cy.tt3, cy.pt3, cy.tt5, cy.pt5, cy.tt7, cy.npr, cy.v9, cy.thermal_efficiency, cy.propulsive_efficiency,
    ]
}
