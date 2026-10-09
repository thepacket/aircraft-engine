//! Free-turbine turboprop: Pratt & Whitney Canada PT6A-114A (675 shp) as
//! fitted to the Cessna 208B Grand Caravan.
//!
//! Gas generator: 3 axial + 1 centrifugal compressor stages, reverse-flow
//! annular combustor, single-stage compressor turbine (CT). A free power
//! turbine (PT) drives the propeller through a 15:1 reduction gearbox. The
//! inter-turbine temperature (ITT) is measured between CT and PT (station 45).
//!
//! Sources: FAA TCDS E4EA, Cessna 208B POH (limits, indications, start),
//! P&WC PT6A-114A maintenance / operating data (published figures).

use crate::atmosphere::{inlet as inlet_conditions, isa, Inlet};
use crate::cycle::{nozzle, CP_AIR, CP_GAS, GAMMA_GAS};
use crate::logger::DataLogger;
use crate::propeller::{governor_step, prop_forces, PropForces, PropSpec};
use crate::spec::interp;
use serde::{Deserialize, Serialize};

const TAU: f64 = std::f64::consts::TAU;
const FTLB: f64 = 1.355818; // N m per ft lb
const HP: f64 = 745.7;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbopropSpec {
    pub name: String,
    pub manufacturer: String,
    pub application: String,
    pub rating: TpRating,
    pub design_point: TpDesign,
    pub control: TpControl,
    pub start: TpStart,
    pub oil: TpOil,
    pub propeller: PropSpec,
    pub gearbox: Gearbox,
    pub limits: TpLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpRating {
    /// Takeoff / max continuous shaft power, SL ISA (W)
    pub shaft_power_w: f64,
    pub flat_rating_delta_isa_k: f64,
    pub ng_100pct_rpm: f64,
    pub np_100pct_rpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpDesign {
    pub mass_flow_kg_s: f64,
    pub compressor_pressure_ratio: f64,
    pub eta_compressor: f64,
    pub eta_burner: f64,
    pub eta_ct: f64,
    pub eta_pt: f64,
    pub eta_mechanical: f64,
    pub burner_pressure_loss: f64,
    pub cooling_bleed_fraction: f64,
    pub power_extraction_w: f64,
    pub fuel_lhv_j_kg: f64,
    /// Exhaust: total pressure at PT exit over ambient (duct loss)
    pub exhaust_pressure_ratio: f64,
    pub flow_exponent: f64,
    pub pr_exponent: f64,
    pub efficiency_falloff: f64,
    /// PT guide-vane flow function shape below choke: phi = ((1 - PR^-k)/(1 - PRcrit^-k))^exp
    pub ngv_flow_exponent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpControl {
    /// Condition lever idle settings (corrected Ng %)
    pub ng_low_idle_pct: f64,
    pub ng_high_idle_pct: f64,
    /// Power lever to Ng mapping exponent above the idle range
    pub lever_exponent: f64,
    /// Fuel control: Ng governor gain (kg/s per %), Wf/P3 schedules
    pub governor_gain_kg_s_per_pct: f64,
    pub accel_wf_p3_vs_ngc: Vec<[f64; 2]>,
    pub decel_wf_p3_vs_ngc: Vec<[f64; 2]>,
    pub min_fuel_kg_s: f64,
    pub fuel_lag_s: f64,
    /// Propeller governor: rpm range of the prop lever and gain
    pub np_min_rpm: f64,
    pub np_max_rpm: f64,
    pub governor_gain_deg_per_rpm_s: f64,
    /// Np overspeed governor: fuel topping above this rpm
    pub np_overspeed_rpm: f64,
    /// Beta range: power lever below this is beta/reverse
    pub beta_lever: f64,
    /// Max Ng in reverse (%)
    pub ng_reverse_max_pct: f64,
    /// Gas generator inertia (kg m^2)
    pub ng_inertia_kg_m2: f64,
    /// Feather / unfeather rate (deg/s)
    pub feather_rate_deg_s: f64,
    /// Surge margin at design (%)
    pub surge_margin_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpStart {
    pub starter_max_ng_pct: f64,
    pub starter_tau_s: f64,
    pub min_light_ng_pct: f64,
    pub light_off_delay_s: f64,
    pub starter_cutout_ng_pct: f64,
    pub max_light_altitude_m: f64,
    pub windmill_ng_per_mach: f64,
    pub start_fuel_schedule: Vec<[f64; 2]>,
    pub start_accel_vs_ng: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpOil {
    pub capacity_qt: f64,
    pub full_qt: f64,
    pub press_gain_psi_per_pct: f64,
    pub press_offset_psi: f64,
    pub temp_rise_idle_k: f64,
    pub temp_rise_takeoff_k: f64,
    pub temp_tau_s: f64,
    pub starvation_qt: f64,
    pub seizure_time_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gearbox {
    pub ratio: f64,
    pub efficiency: f64,
    /// Propeller + PT + shafting inertia referred to the propeller shaft (kg m^2)
    pub inertia_kg_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpLimits {
    pub torque_max_nm: f64,
    pub itt_max_c: f64,
    pub itt_transient_c: f64,
    pub itt_start_c: f64,
    pub ng_max_pct: f64,
    pub np_max_rpm: f64,
    pub np_transient_rpm: f64,
    pub oil_press_min_psi: f64,
    pub oil_press_min_idle_psi: f64,
    pub oil_press_max_psi: f64,
    pub oil_temp_max_c: f64,
}

impl TurbopropSpec {
    pub fn pt6a_114a() -> Self {
        TurbopropSpec {
            name: "PT6A-114A".into(),
            manufacturer: "Pratt & Whitney Canada".into(),
            application: "Cessna 208B Grand Caravan".into(),
            rating: TpRating { shaft_power_w: 675.0 * HP, flat_rating_delta_isa_k: 20.0, ng_100pct_rpm: 37_468.0, np_100pct_rpm: 1_900.0 },
            design_point: TpDesign {
                mass_flow_kg_s: 3.0,
                compressor_pressure_ratio: 9.2,
                eta_compressor: 0.76,
                eta_burner: 0.99,
                eta_ct: 0.85,
                eta_pt: 0.86,
                eta_mechanical: 0.99,
                burner_pressure_loss: 0.04,
                cooling_bleed_fraction: 0.09,
                power_extraction_w: 20000.0,
                fuel_lhv_j_kg: 43.0e6,
                exhaust_pressure_ratio: 1.03,
                flow_exponent: 1.3,
                pr_exponent: 1.9,
                efficiency_falloff: 0.45,
                ngv_flow_exponent: 0.08,
            },
            control: TpControl {
                ng_low_idle_pct: 52.0,
                ng_high_idle_pct: 65.0,
                lever_exponent: 1.2,
                governor_gain_kg_s_per_pct: 0.0006,
                accel_wf_p3_vs_ngc: vec![[50.0, 0.30], [60.0, 0.30], [70.0, 0.31], [80.0, 0.32], [90.0, 0.33], [100.0, 0.34], [105.0, 0.34]],
                decel_wf_p3_vs_ngc: vec![[50.0, 0.14], [70.0, 0.14], [100.0, 0.16], [105.0, 0.16]],
                min_fuel_kg_s: 0.006,
                fuel_lag_s: 0.2,
                np_min_rpm: 1_600.0,
                np_max_rpm: 1_900.0,
                governor_gain_deg_per_rpm_s: 0.08,
                np_overspeed_rpm: 2_000.0,
                beta_lever: 0.12,
                ng_reverse_max_pct: 88.0,
                ng_inertia_kg_m2: 0.035,
                feather_rate_deg_s: 20.0,
                surge_margin_pct: 18.0,
            },
            start: TpStart {
                starter_max_ng_pct: 20.0,
                starter_tau_s: 5.0,
                min_light_ng_pct: 12.0,
                light_off_delay_s: 2.0,
                starter_cutout_ng_pct: 50.0,
                max_light_altitude_m: 6_100.0,
                windmill_ng_per_mach: 20.0,
                start_fuel_schedule: vec![[12.0, 0.0045], [20.0, 0.006], [30.0, 0.0085], [40.0, 0.011], [50.0, 0.013], [54.0, 0.0135]],
                start_accel_vs_ng: vec![[12.0, 1.2], [18.0, 2.2], [25.0, 2.8], [35.0, 3.6], [45.0, 5.0], [50.0, 6.0], [54.0, 5.8]],
            },
            oil: TpOil { capacity_qt: 9.5, full_qt: 8.5, press_gain_psi_per_pct: 1.15, press_offset_psi: -13.0, temp_rise_idle_k: 45.0, temp_rise_takeoff_k: 70.0, temp_tau_s: 200.0, starvation_qt: 2.5, seizure_time_s: 90.0 },
            propeller: PropSpec {
                diameter_m: 2.69, // 106 in, 3 blades
                blades: 3,
                blade_area_m2: 0.400,
                fixed_pitch_deg: None,
                beta_min_deg: 6.0,
                beta_max_deg: 55.0,
                beta_feather_deg: 86.0,
                beta_reverse_deg: -11.0,
                cl_alpha_per_rad: 5.5,
                cl_max: 1.35,
                alpha_zero_lift_deg: -2.0,
                alpha_stall_deg: 14.0,
                alpha_stall_neg_deg: 10.0,
                cl_max_neg: 0.9,
                cd0: 0.012,
                k_induced: 0.05,
                inertia_kg_m2: 16.0,
            },
            gearbox: Gearbox { ratio: 15.0, efficiency: 0.98, inertia_kg_m2: 18.0 },
            limits: TpLimits {
                torque_max_nm: 1_970.0 * FTLB,
                itt_max_c: 805.0,
                itt_transient_c: 865.0,
                itt_start_c: 1_090.0,
                ng_max_pct: 101.6,
                np_max_rpm: 1_900.0,
                np_transient_rpm: 2_090.0,
                oil_press_min_psi: 85.0,
                oil_press_min_idle_psi: 40.0,
                oil_press_max_psi: 105.0,
                oil_temp_max_c: 99.0,
            },
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("spec serialises")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TpControls {
    /// Power lever: -0.3 (max reverse) .. 0 .. beta range .. 1 (max)
    pub power_lever: f64,
    /// Propeller lever: 0 feather, 0.1..1 = min..max governed rpm
    pub prop_lever: f64,
    /// Condition lever: 0 cutoff, 1 low idle, 2 high idle
    pub condition_lever: u8,
    pub starter: bool,
    pub ignition: bool,
    /// Compressor bleed: generator load etc. not modelled; cabin bleed fraction
    pub bleed_air: bool,
    /// Inertial separator (ice protection): small power loss
    pub inertial_separator: bool,
}

impl Default for TpControls {
    fn default() -> Self {
        TpControls { power_lever: 0.0, prop_lever: 0.0, condition_lever: 0, starter: false, ignition: false, bleed_air: false, inertial_separator: false }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TpEnvironment {
    pub altitude_m: f64,
    pub mach: f64,
    pub delta_isa_k: f64,
}

impl Default for TpEnvironment {
    fn default() -> Self {
        TpEnvironment { altitude_m: 0.0, mach: 0.0, delta_isa_k: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TpFaults {
    pub starter_inop: bool,
    /// Weak battery: starter reaches only ~12 % Ng (hot start risk)
    pub starter_weak: bool,
    pub starter_early_cutout: bool,
    pub ignition_fail: bool,
    pub start_fuel_factor: f64,
    pub fuel_pump_fail: bool,
    pub oil_leak_qt_per_min: f64,
    pub fire: bool,
    /// Propeller governor failed: blade angle frozen (overspeed / underspeed)
    pub prop_governor_fail: bool,
    pub compressor_damage_pct: f64,
    pub fod: bool,
    pub itt_probe_fail: bool,
    pub torque_sensor_fail: bool,
    /// Chip detector light (metal in the oil)
    pub chip_detector: bool,
    pub trigger_surge: bool,
    pub trigger_flameout: bool,
    pub fire_bottle: bool,
}

impl TpFaults {
    pub fn none() -> Self {
        TpFaults { start_fuel_factor: 1.0, ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TpMode {
    Off,
    Motoring,
    Starting,
    Running,
    Shutdown,
}

/// Gas-generator cycle result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TpCycle {
    pub w2: f64,
    pub wf: f64,
    pub pr: f64,
    pub tt3: f64,
    pub pt3: f64,
    pub t4: f64,
    pub pt4: f64,
    pub tt45: f64,
    pub pt45: f64,
    pub tt5: f64,
    pub pt5: f64,
    pub ct_pr: f64,
    pub pt_pr: f64,
    pub compressor_power_w: f64,
    pub ct_power_w: f64,
    pub pt_power_w: f64,
    pub shaft_power_w: f64,
    pub jet_thrust_n: f64,
    pub v9: f64,
    pub a45_required: f64,
    pub ps3_kpa: f64,
    pub thermal_efficiency: f64,
}

fn compress(tt: f64, pr: f64, eta: f64) -> f64 {
    tt * (1.0 + (pr.powf(0.4 / 1.4) - 1.0) / eta)
}

fn turbine_pr(tt_in: f64, dt: f64, eta: f64) -> f64 {
    let k = (GAMMA_GAS - 1.0) / GAMMA_GAS;
    (1.0 - (dt / tt_in) / eta).max(0.02).powf(-1.0 / k)
}

fn fuel_for_t4(spec: &TurbopropSpec, w: f64, t3: f64, t4: f64) -> f64 {
    let d = &spec.design_point;
    (w * CP_GAS * (t4 - t3) / (d.eta_burner * d.fuel_lhv_j_kg - CP_GAS * t4)).max(0.0)
}

fn t4_for_fuel(spec: &TurbopropSpec, w: f64, t3: f64, wf: f64) -> f64 {
    let d = &spec.design_point;
    ((wf * d.eta_burner * d.fuel_lhv_j_kg + w * CP_GAS * t3) / ((w + wf) * CP_GAS)).min(2_300.0)
}

/// Gas generator at corrected speed `ngc` (fraction) and turbine inlet
/// temperature `t4`, with the power turbine expanding to the exhaust.
/// `np_factor` scales PT efficiency with propeller speed (speed ratio effect).
pub fn tp_cycle(spec: &TurbopropSpec, inlet: &Inlet, ngc: f64, t4: f64, combustion: bool, np_factor: f64, bleed: f64) -> TpCycle {
    let d = &spec.design_point;
    let n = ngc.max(0.0);
    let corr = inlet.delta / inlet.theta.sqrt();
    let w2 = (d.mass_flow_kg_s * corr * n.powf(d.flow_exponent)).max(1e-3);
    let pr = 1.0 + (d.compressor_pressure_ratio - 1.0) * n.powf(d.pr_exponent);
    let fo = |eta: f64| eta * (1.0 - d.efficiency_falloff * (1.0 - n.min(1.0)).powi(2));
    let tt3 = compress(inlet.tt2_k, pr, fo(d.eta_compressor));
    let pt3 = inlet.pt2_pa * pr;
    let fc = d.cooling_bleed_fraction + bleed;
    let w_b = w2 * (1.0 - fc);
    let t4 = if combustion { t4.max(tt3) } else { tt3 };
    let wf = if combustion { fuel_for_t4(spec, w_b, tt3, t4) } else { 0.0 };
    let pt4 = pt3 * (1.0 - d.burner_pressure_loss);
    let w4 = w_b + wf;
    let w45 = w2 * (1.0 - bleed) + wf;
    let comp_power = w2 * CP_AIR * (tt3 - inlet.tt2_k) + d.power_extraction_w;
    let ct_power = if combustion { comp_power / d.eta_mechanical } else { 0.0 };
    let dt_ct = ct_power / (w4 * CP_GAS);
    let tt45_u = t4 - dt_ct;
    let ct_pr = if combustion { turbine_pr(t4, dt_ct, fo(d.eta_ct)) } else { 1.0 };
    let pt45 = pt4 / ct_pr;
    let tt45 = (w4 * CP_GAS * tt45_u + d.cooling_bleed_fraction * w2 * CP_AIR * tt3) / (w45 * CP_GAS);
    // Power turbine expands to the exhaust duct pressure
    let p0 = inlet.ambient.pressure_pa;
    let pt5 = d.exhaust_pressure_ratio * p0;
    let pt_pr = (pt45 / pt5).max(1.0);
    let k = (GAMMA_GAS - 1.0) / GAMMA_GAS;
    let eta_pt = fo(d.eta_pt) * np_factor.clamp(0.0, 1.0);
    let dt_pt = if combustion { eta_pt * tt45 * (1.0 - pt_pr.powf(-k)) } else { 0.0 };
    let tt5 = tt45 - dt_pt;
    let pt_power = w45 * CP_GAS * dt_pt;
    let shaft_power = (pt_power * spec.gearbox.efficiency).max(0.0);
    let ex = nozzle(w45, tt5, pt5, p0, GAMMA_GAS, CP_GAS);
    let jet_thrust = w45 * ex.exit_velocity_m_s;
    // PT nozzle guide vane continuity. The vanes choke once the power turbine
    // pressure ratio exceeds the critical value; below that the flow function
    // falls with the pressure ratio, so at idle P45 stays above the exhaust
    // pressure and the power turbine keeps delivering some torque.
    let crit = ((GAMMA_GAS + 1.0) / 2.0).powf(GAMMA_GAS / (GAMMA_GAS - 1.0));
    let phi = if pt_pr <= 1.0001 { 1e-6 } else { ((1.0 - pt_pr.powf(-k)) / (1.0 - crit.powf(-k))).powf(spec.design_point.ngv_flow_exponent).min(1.0) };
    let a45_required = w45 * tt45.sqrt() / (pt45 * phi);
    TpCycle {
        w2,
        wf,
        pr,
        tt3,
        pt3,
        t4,
        pt4,
        tt45,
        pt45,
        tt5,
        pt5,
        ct_pr,
        pt_pr,
        compressor_power_w: comp_power,
        ct_power_w: ct_power,
        pt_power_w: pt_power,
        shaft_power_w: shaft_power,
        jet_thrust_n: jet_thrust,
        v9: ex.exit_velocity_m_s,
        a45_required,
        ps3_kpa: pt3 * 0.97 / 1000.0,
        thermal_efficiency: if wf > 0.0 { ((shaft_power + 0.5 * w45 * ex.exit_velocity_m_s.powi(2)) / (wf * d.fuel_lhv_j_kg)).clamp(0.0, 1.0) } else { 0.0 },
    }
}

/// T4 at which the CT exit flow fits the PT guide vanes of effective area `a45`.
pub fn tp_solve_t4(spec: &TurbopropSpec, inlet: &Inlet, ngc: f64, a45: f64, np_factor: f64, bleed: f64) -> f64 {
    let probe = tp_cycle(spec, inlet, ngc, inlet.tt2_k, false, np_factor, bleed);
    let f = |t4: f64| tp_cycle(spec, inlet, ngc, t4, true, np_factor, bleed).a45_required - a45;
    let (mut lo, mut hi) = (probe.tt3 + 20.0, 2_400.0);
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
pub struct TpSizing {
    pub a45_effective: f64,
    pub t4_takeoff_k: f64,
    pub takeoff: TpCycle,
}

/// Size the PT guide vanes so that 100 % Ng gives the rated shaft power at
/// sea level, static, ISA, with the propeller at 100 % Np.
pub fn tp_size(spec: &TurbopropSpec) -> TpSizing {
    let inlet = inlet_conditions(isa(0.0, 0.0), 0.0);
    let target = spec.rating.shaft_power_w;
    let probe = tp_cycle(spec, &inlet, 1.0, inlet.tt2_k, false, 1.0, 0.0);
    let (mut lo, mut hi) = (probe.tt3 + 50.0, 2_200.0);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if tp_cycle(spec, &inlet, 1.0, mid, true, 1.0, 0.0).shaft_power_w < target { lo = mid } else { hi = mid }
        if hi - lo < 0.005 { break }
    }
    let t4 = 0.5 * (lo + hi);
    let r = tp_cycle(spec, &inlet, 1.0, t4, true, 1.0, 0.0);
    TpSizing { a45_effective: r.a45_required, t4_takeoff_k: t4, takeoff: r }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpState {
    pub kind: String,
    pub time_s: f64,
    pub mode: TpMode,
    pub controls: TpControls,
    pub environment: TpEnvironment,
    pub faults: TpFaults,
    pub inlet: Inlet,
    pub ng_pct: f64,
    pub ng_rpm: f64,
    pub ng_corrected_pct: f64,
    pub ng_command_pct: f64,
    pub ng_idle_pct: f64,
    pub np_rpm: f64,
    pub np_pct: f64,
    pub np_target_rpm: f64,
    pub torque_nm: f64,
    pub torque_ftlb: f64,
    pub torque_pct: f64,
    pub torque_valid: bool,
    pub itt_c: f64,
    pub itt_valid: bool,
    pub itt_gas_c: f64,
    pub t4_k: f64,
    pub t4_steady_k: f64,
    pub shaft_power_w: f64,
    pub shaft_power_hp: f64,
    pub power_pct: f64,
    pub fuel_flow_kg_s: f64,
    pub fuel_flow_pph: f64,
    pub fuel_flow_steady_kg_s: f64,
    pub fuel_command_kg_s: f64,
    pub fuel_accel_limit_kg_s: f64,
    pub fuel_decel_limit_kg_s: f64,
    pub wf_p3_ratio: f64,
    pub ps3_kpa: f64,
    pub fuel_used_kg: f64,
    pub sfc_lb_shp_h: f64,
    pub prop: PropForces,
    pub prop_beta_deg: f64,
    pub feathered: bool,
    pub governing: bool,
    pub thrust_n: f64,
    pub thrust_lbf: f64,
    pub jet_thrust_n: f64,
    pub oil_pressure_psi: f64,
    pub oil_temperature_c: f64,
    pub oil_quantity_qt: f64,
    pub vib: f64,
    pub starter_engaged: bool,
    pub ignition: bool,
    pub lit: bool,
    pub surge_margin_pct: f64,
    pub surge: bool,
    pub surge_count: u32,
    pub flameout_count: u32,
    pub fire_warning: bool,
    pub seized: bool,
    pub cycle: TpCycle,
    pub run_time_s: f64,
    pub warnings: Vec<String>,
    pub logger_rows: usize,
    pub logger_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurbopropEngine {
    pub spec: TurbopropSpec,
    pub sizing: TpSizing,
    pub controls: TpControls,
    pub environment: TpEnvironment,
    pub faults: TpFaults,
    pub logger: DataLogger,
    time_s: f64,
    mode: TpMode,
    ng: f64,
    np: f64,
    beta: f64,
    ng_cmd: f64,
    ng_idle: f64,
    lit: bool,
    fuel_on_since: Option<f64>,
    starter_engaged: bool,
    wf: f64,
    wf_cmd: f64,
    wf_max: f64,
    wf_min: f64,
    t4: f64,
    t4_ss: f64,
    ps3_kpa: f64,
    itt_gas_k: f64,
    itt_sensed_k: f64,
    ff_sensed: f64,
    ff_steady: f64,
    fuel_used_kg: f64,
    torque_nm: f64,
    shaft_power_w: f64,
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
    governing: bool,
    prop: PropForces,
    run_time_s: f64,
    seed: u64,
    idle_fuel_kg_s: f64,
    last_cycle: TpCycle,
    warnings: Vec<String>,
    max_substep_s: f64,
}

pub const TP_LOG_COLUMNS: &[&str] = &[
    "time_s", "mode", "power_lever", "prop_lever", "condition_lever", "starter", "ignition", "bleed_air",
    "altitude_m", "mach", "delta_isa_k", "tt2_k", "pt2_pa",
    "ng_pct", "ng_corrected_pct", "ng_command_pct", "np_rpm", "np_target_rpm", "torque_ftlb", "torque_pct",
    "itt_c", "itt_gas_c", "t4_k", "t4_steady_k", "shaft_power_hp", "power_pct",
    "fuel_flow_kg_s", "fuel_flow_pph", "fuel_command_kg_s", "fuel_accel_limit_kg_s", "fuel_decel_limit_kg_s", "wf_p3_ratio", "ps3_kpa", "fuel_used_kg", "sfc_lb_shp_h",
    "prop_beta_deg", "prop_efficiency", "thrust_n", "jet_thrust_n", "oil_pressure_psi", "oil_temperature_c", "oil_quantity_qt", "vib",
    "surge_margin_pct", "surge", "lit", "fire",
    "w2_kg_s", "compressor_pr", "tt3_k", "pt3_pa", "tt45_k", "pt45_pa", "tt5_k", "thermal_eff",
];

impl TurbopropEngine {
    pub fn new(spec: TurbopropSpec) -> Self {
        let sizing = tp_size(&spec);
        let env = TpEnvironment::default();
        let inl = inlet_conditions(isa(env.altitude_m, env.delta_isa_k), env.mach);
        let cold = tp_cycle(&spec, &inl, 0.0, inl.tt2_k, false, 1.0, 0.0);
        let idle_fuel_kg_s = {
            let t4 = tp_solve_t4(&spec, &inl, spec.control.ng_low_idle_pct / 100.0, sizing.a45_effective, 0.6, 0.0);
            tp_cycle(&spec, &inl, spec.control.ng_low_idle_pct / 100.0, t4, true, 0.6, 0.0).wf
        };
        let columns = TP_LOG_COLUMNS.iter().map(|s| s.to_string()).collect();
        let full = spec.oil.full_qt;
        let feather = spec.propeller.beta_feather_deg;
        TurbopropEngine {
            spec,
            sizing,
            controls: TpControls::default(),
            environment: env,
            faults: TpFaults::none(),
            logger: DataLogger::new(10.0, columns),
            time_s: 0.0,
            mode: TpMode::Off,
            ng: 0.0,
            np: 0.0,
            beta: feather,
            ng_cmd: 0.0,
            ng_idle: 52.0,
            lit: false,
            fuel_on_since: None,
            starter_engaged: false,
            wf: 0.0,
            wf_cmd: 0.0,
            wf_max: 0.0,
            wf_min: 0.0,
            t4: inl.tt2_k,
            t4_ss: inl.tt2_k,
            ps3_kpa: inl.pt2_pa / 1000.0,
            itt_gas_k: inl.tt2_k,
            itt_sensed_k: inl.tt2_k,
            ff_sensed: 0.0,
            ff_steady: 0.0,
            fuel_used_kg: 0.0,
            torque_nm: 0.0,
            shaft_power_w: 0.0,
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
            governing: false,
            prop: PropForces::default(),
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
        *self = TurbopropEngine::new(spec);
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

    /// Jump to a stabilised running condition at the given power lever.
    pub fn set_running(&mut self, power_lever: f64) {
        self.controls.power_lever = power_lever.clamp(0.0, 1.0);
        self.controls.condition_lever = self.controls.condition_lever.max(1);
        self.controls.prop_lever = if self.controls.prop_lever < 0.1 { 1.0 } else { self.controls.prop_lever };
        self.controls.starter = false;
        self.seized = false;
        let inl = self.inlet();
        self.ng_idle = self.idle_ng_corrected() * inl.theta.sqrt();
        self.ng = self.ng_command_corrected(&inl) * inl.theta.sqrt();
        self.ng_cmd = self.ng;
        self.mode = TpMode::Running;
        self.lit = true;
        self.np = self.np_target();
        self.beta = 25.0;
        let t4 = tp_solve_t4(&self.spec, &inl, self.ng / inl.theta.sqrt() / 100.0, self.sizing.a45_effective, self.np_factor(), self.bleed());
        let r = tp_cycle(&self.spec, &inl, self.ng / inl.theta.sqrt() / 100.0, t4, true, self.np_factor(), self.bleed());
        self.t4 = t4;
        self.t4_ss = t4;
        self.wf = r.wf;
        self.wf_cmd = r.wf;
        self.ff_sensed = r.wf;
        self.ff_steady = r.wf;
        self.itt_gas_k = r.tt45;
        self.itt_sensed_k = r.tt45;
        self.oil_temp_k = self.oil_temp_target(&inl);
        self.last_cycle = r;
        // Let the propeller governor settle
        for _ in 0..800 {
            self.substep(0.01);
        }
        self.time_s = 0.0;
        self.run_time_s = 0.0;
        self.fuel_used_kg = 0.0;
        self.logger.clear();
    }

    fn bleed(&self) -> f64 {
        (if self.controls.bleed_air { 0.03 } else { 0.0 }) + (if self.controls.inertial_separator { 0.01 } else { 0.0 })
    }

    fn np_factor(&self) -> f64 {
        // Power turbine efficiency falls away from its design speed ratio
        let r = self.np / self.spec.rating.np_100pct_rpm;
        (1.0 - 0.6 * (1.0 - r).powi(2)).clamp(0.15, 1.0)
    }

    fn np_target(&self) -> f64 {
        let c = &self.spec.control;
        let l = self.controls.prop_lever.clamp(0.0, 1.0);
        if l < 0.1 { c.np_min_rpm } else { c.np_min_rpm + (c.np_max_rpm - c.np_min_rpm) * ((l - 0.1) / 0.9) }
    }

    fn idle_ng_corrected(&self) -> f64 {
        let c = &self.spec.control;
        let base = if self.controls.condition_lever >= 2 { c.ng_high_idle_pct } else { c.ng_low_idle_pct };
        base + 1.5 * (self.environment.altitude_m / 1000.0).max(0.0)
    }

    fn ng_command_corrected(&self, inl: &Inlet) -> f64 {
        let c = &self.spec.control;
        let st = inl.theta.sqrt();
        let idle_c = self.ng_idle / st;
        let max_c = (100.0f64).min(self.spec.limits.ng_max_pct / st);
        let pl = self.controls.power_lever;
        if pl < 0.0 {
            // Reverse: Ng rises with reverse lever travel
            let rev = (-pl / 0.3).clamp(0.0, 1.0);
            return idle_c + (c.ng_reverse_max_pct / st - idle_c) * rev;
        }
        if pl <= c.beta_lever {
            return idle_c;
        }
        let x = ((pl - c.beta_lever) / (1.0 - c.beta_lever)).clamp(0.0, 1.0);
        idle_c + (max_c - idle_c) * x.powf(c.lever_exponent)
    }

    fn oil_temp_target(&self, inl: &Inlet) -> f64 {
        let o = &self.spec.oil;
        if matches!(self.mode, TpMode::Running | TpMode::Starting) {
            let load = ((self.ng - 52.0) / 48.0).clamp(0.0, 1.0);
            inl.tt2_k + o.temp_rise_idle_k + (o.temp_rise_takeoff_k - o.temp_rise_idle_k) * load
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
        let a45 = self.sizing.a45_effective;
        self.warnings.clear();
        self.faults.trigger_surge = false;
        self.faults.trigger_flameout = false;
        self.faults.fire_bottle = false;
        self.ng_idle = self.idle_ng_corrected() * st;

        let ng_wm = (sp.windmill_ng_per_mach * self.environment.mach).clamp(0.0, 25.0);
        let starter_ok = c.starter && !f.starter_inop && !self.seized;
        let starter_max = if f.starter_weak { 12.0 } else { sp.starter_max_ng_pct };
        let starter_cutout = if f.starter_early_cutout { 25.0 } else { sp.starter_cutout_ng_pct };
        let fuel_available = c.condition_lever >= 1 && !f.fuel_pump_fail;
        let ignition_on = c.ignition || starter_ok;
        let light_possible = self.ng >= sp.min_light_ng_pct && self.environment.altitude_m <= sp.max_light_altitude_m && !f.ignition_fail && ignition_on;

        // ---- Mode transitions ---------------------------------------------
        match self.mode {
            TpMode::Off => {
                if starter_ok { self.mode = TpMode::Motoring }
                else if fuel_available && self.ng >= sp.min_light_ng_pct { self.mode = TpMode::Starting; self.fuel_on_since = Some(self.time_s) }
            }
            TpMode::Motoring => {
                if fuel_available && self.ng >= sp.min_light_ng_pct { self.mode = TpMode::Starting; self.fuel_on_since = Some(self.time_s) }
                else if !starter_ok { self.mode = TpMode::Off }
            }
            TpMode::Starting => {
                if !fuel_available || self.seized { self.mode = TpMode::Shutdown; self.lit = false; self.fuel_on_since = None }
                else if self.lit && self.ng >= ctl.ng_low_idle_pct * st - 0.3 { self.mode = TpMode::Running; self.ng_cmd = self.ng; self.hung_timer = 0.0 }
            }
            TpMode::Running => {
                if !fuel_available || self.seized { self.mode = TpMode::Shutdown; self.lit = false; self.fuel_on_since = None }
                else if !self.lit { self.mode = TpMode::Starting; self.fuel_on_since = Some(self.time_s) }
                else if self.ng < ctl.ng_low_idle_pct * st - 8.0 { self.mode = TpMode::Starting; self.fuel_on_since = Some(self.time_s - sp.light_off_delay_s); self.surge_timer = 0.0 }
            }
            TpMode::Shutdown => {
                if fuel_available && self.ng >= sp.min_light_ng_pct && !self.seized { self.mode = TpMode::Starting; self.fuel_on_since = Some(self.time_s) }
                else if starter_ok && self.ng < starter_max { self.mode = TpMode::Motoring }
                else if self.ng < 1.0 { self.mode = TpMode::Off }
            }
        }
        self.starter_engaged = starter_ok && self.ng < starter_cutout && self.mode != TpMode::Running;
        let starter_rate0 = starter_max / sp.starter_tau_s;
        let starter_assist = if self.starter_engaged { (starter_rate0 * (1.0 - self.ng / (starter_max * 2.8))).max(0.0) } else { 0.0 };
        let drag_rate = |ng: f64| (ng - ng_wm).max(0.0) / 10.0;

        // ---- Gas generator dynamics -----------------------------------------
        let combustion;
        let mut wf_dyn = 0.0;
        let ngc = self.ng / st / 100.0;
        let np_factor = self.np_factor();
        let bleed = self.bleed();
        match self.mode {
            TpMode::Off | TpMode::Motoring | TpMode::Shutdown => {
                combustion = false;
                if self.seized { self.ng += (0.0 - self.ng) / 3.0 * h }
                else if self.starter_engaged { self.ng += (starter_assist - drag_rate(self.ng)) * h }
                else { self.ng += (ng_wm - self.ng) / (if self.mode == TpMode::Shutdown { 15.0 } else { 10.0 }) * h }
                self.t4 = inl.tt2_k;
                self.t4_ss = inl.tt2_k;
                self.wf = 0.0;
                self.wf_cmd = 0.0;
                self.wf_max = 0.0;
                self.wf_min = 0.0;
            }
            TpMode::Starting => {
                if let Some(t0) = self.fuel_on_since {
                    if !self.lit && self.time_s - t0 >= sp.light_off_delay_s && light_possible { self.lit = true }
                    if !self.lit && self.time_s - t0 > sp.light_off_delay_s + 8.0 {
                        if f.ignition_fail || !ignition_on { self.warnings.push("NO LIGHT: ignition inoperative / off (wet start, fuel accumulating)".into()) }
                        else if self.ng < sp.min_light_ng_pct { self.warnings.push(format!("NO LIGHT: Ng {:.0}% below {:.0}%", self.ng, sp.min_light_ng_pct)) }
                    }
                }
                combustion = self.lit;
                let sched_idle = interp(&sp.start_fuel_schedule, ctl.ng_low_idle_pct).max(1e-6);
                let scale = self.idle_fuel_kg_s * inl.delta / sched_idle * f.start_fuel_factor.max(0.1);
                wf_dyn = interp(&sp.start_fuel_schedule, self.ng / st) * scale;
                self.wf = wf_dyn;
                self.wf_cmd = wf_dyn;
                let mut ng_dot = starter_assist - drag_rate(self.ng);
                if self.lit { ng_dot += interp(&sp.start_accel_vs_ng, self.ng / st) * (0.4 + 0.6 * f.start_fuel_factor).max(0.2) }
                self.ng += ng_dot * h;
                if self.lit && self.ng < sp.min_light_ng_pct - 3.0 { self.lit = false; self.fuel_on_since = Some(self.time_s) }
                if self.lit && !self.starter_engaged && self.ng < self.ng_idle - 3.0 {
                    if ng_dot < 0.15 { self.hung_timer += h } else { self.hung_timer = 0.0 }
                    if self.hung_timer > 6.0 { self.warnings.push(format!("HUNG START: Ng stalled at {:.0}%", self.ng)) }
                }
            }
            TpMode::Running => {
                combustion = true;
                let ngc_cmd = self.ng_command_corrected(&inl);
                self.ng_cmd = ngc_cmd * st;
                // Np overspeed governor: fuel topping, proportional above max Np
                if self.np > ctl.np_overspeed_rpm {
                    let cut = (self.np - ctl.np_overspeed_rpm) / 10.0;
                    self.ng_cmd = self.ng_cmd.min(self.ng - cut).max(self.ng_idle);
                }
                let t4_ss = tp_solve_t4(&spec, &inl, ngc, a45, np_factor, bleed);
                let ss = tp_cycle(&spec, &inl, ngc, t4_ss, true, np_factor, bleed);
                self.t4_ss = t4_ss;
                self.ff_steady = ss.wf;
                self.ps3_kpa = ss.ps3_kpa;
                let demand = ss.wf + ctl.governor_gain_kg_s_per_pct * (self.ng_cmd - self.ng);
                let ngc_pct = self.ng / st;
                self.wf_max = interp(&ctl.accel_wf_p3_vs_ngc, ngc_pct) * self.ps3_kpa / 3600.0;
                self.wf_min = (interp(&ctl.decel_wf_p3_vs_ngc, ngc_pct) * self.ps3_kpa / 3600.0).max(ctl.min_fuel_kg_s.min(ss.wf));
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
                self.t4 = t4_for_fuel(&spec, w_b, ss.tt3, self.wf);
                let ratio = (self.t4 / self.t4_ss).max(0.2);
                let p_excess = ss.ct_power_w * (ratio - 1.0);
                let w = (self.ng / 100.0 * spec.rating.ng_100pct_rpm * TAU / 60.0).max(50.0);
                let mut a = p_excess / (ctl.ng_inertia_kg_m2 * w);
                if self.surge_timer > 0.0 { a -= 0.06 * w }
                self.ng += a / (spec.rating.ng_100pct_rpm * TAU / 60.0) * 100.0 * h;
                self.ng = self.ng.max(ng_wm);
                let sm_avail = ctl.surge_margin_pct * (0.55 + 0.45 * ngc.min(1.0)) - f.compressor_damage_pct - if f.fod { 4.0 } else { 0.0 };
                self.sm = ((1.0 + sm_avail / 100.0) / ratio.powf(0.3) - 1.0) * 100.0;
                if self.surge_timer <= 0.0 && (self.sm < 0.0 || f.trigger_surge) { self.surge_timer = 1.5; self.surge_recovery_s = 4.0; self.surge_count += 1 }
                if f.trigger_flameout || self.wf < 0.5 * ctl.min_fuel_kg_s { self.lit = false; self.flameout_count += 1 }
            }
        }
        if self.surge_timer > 0.0 { self.surge_timer -= h; self.warnings.push("COMPRESSOR SURGE / STALL".into()) }
        self.ng = self.ng.max(0.0);
        let ngc = self.ng / st / 100.0;

        // ---- Cycle at the actual point --------------------------------------
        let mut r = if combustion {
            if self.mode == TpMode::Starting {
                let probe = tp_cycle(&spec, &inl, ngc, inl.tt2_k, false, np_factor, bleed);
                let w_b = probe.w2 * (1.0 - spec.design_point.cooling_bleed_fraction - bleed);
                self.t4 = t4_for_fuel(&spec, w_b, probe.tt3, wf_dyn);
                self.t4_ss = self.t4;
                self.ff_steady = wf_dyn;
            }
            tp_cycle(&spec, &inl, ngc, self.t4, true, np_factor, bleed)
        } else {
            self.ff_steady = 0.0;
            tp_cycle(&spec, &inl, ngc, inl.tt2_k, false, np_factor, bleed)
        };
        if self.surge_timer > 0.0 { r.shaft_power_w *= 0.3; r.tt45 += 120.0 }
        if f.fod { r.tt45 += 15.0 }
        if self.seized { r.shaft_power_w = 0.0 }

        // ---- Propeller: governor, feather, beta, dynamics ---------------------
        let pr = &spec.propeller;
        let np_target = self.np_target();
        let feather_cmd = c.prop_lever < 0.05;
        let gb = &spec.gearbox;
        let omega_p = self.np / 60.0 * TAU;
        if feather_cmd && !f.prop_governor_fail {
            self.beta += (pr.beta_feather_deg - self.beta).clamp(-ctl.feather_rate_deg_s * h, ctl.feather_rate_deg_s * h);
            self.governing = false;
        } else if c.power_lever < 0.0 && !f.prop_governor_fail {
            // Reverse: blade angle scheduled by the power lever
            let rev = (-c.power_lever / 0.3).clamp(0.0, 1.0);
            let mut target = pr.beta_min_deg + (pr.beta_reverse_deg - pr.beta_min_deg) * rev;
            // Transient overspeed protection: back the blades toward flat pitch
            if self.np > spec.limits.np_transient_rpm { target = target.max(self.beta + 2.0) }
            self.beta += (target - self.beta).clamp(-ctl.feather_rate_deg_s * h, ctl.feather_rate_deg_s * h);
            self.governing = false;
        } else if c.power_lever <= ctl.beta_lever && !f.prop_governor_fail {
            // Beta range: flat pitch, rpm follows the gas generator
            let target = pr.beta_min_deg + 4.0 * (c.power_lever / ctl.beta_lever).clamp(0.0, 1.0);
            self.beta += (target - self.beta).clamp(-ctl.feather_rate_deg_s * h, ctl.feather_rate_deg_s * h);
            self.governing = false;
        } else if !f.prop_governor_fail {
            if self.beta > pr.beta_max_deg {
                // Coming out of feather
                self.beta -= ctl.feather_rate_deg_s * h;
            } else {
                let (b, fine_stop) = governor_step(pr, self.beta, self.np, np_target, ctl.governor_gain_deg_per_rpm_s, h);
                self.beta = b;
                self.governing = !fine_stop;
            }
        }
        self.prop = prop_forces(pr, inl.ambient.density_kg_m3, inl.ambient.speed_of_sound_m_s, inl.true_airspeed_m_s, self.np, self.beta);
        // Power turbine torque at the propeller shaft (limited at low speed: a
        // stalled turbine delivers at most ~2.5x rated torque)
        let q_max = 2.5 * spec.limits.torque_max_nm;
        let q_pt = if omega_p > 5.0 { (r.shaft_power_w / omega_p).min(q_max) } else if r.shaft_power_w > 0.0 { q_max * 0.6 } else { 0.0 };
        let q_fric = 0.002 * spec.limits.torque_max_nm + 0.08 * self.np;
        let domega = (q_pt - self.prop.torque_nm - q_fric) / gb.inertia_kg_m2;
        self.np = (self.np + domega * h / TAU * 60.0).max(0.0);
        // Windmilling prop in flight
        if r.shaft_power_w <= 0.0 && inl.true_airspeed_m_s > 20.0 && self.beta < 70.0 && self.np < 800.0 {
            self.np += 0.4 * (inl.true_airspeed_m_s - 20.0) * h;
        }
        self.torque_nm = if omega_p > 5.0 { q_pt } else { 0.0 };
        self.shaft_power_w = r.shaft_power_w;

        // ---- Gas temperature at the ITT probe -------------------------------
        if combustion { self.itt_gas_k += (r.tt45 - self.itt_gas_k) / 0.3 * h }
        else { let tau = 20.0 + 50.0 / (1.0 + self.ng / 10.0); self.itt_gas_k += (inl.tt2_k - self.itt_gas_k) / tau * h }
        self.itt_sensed_k += (self.itt_gas_k - self.itt_sensed_k) / 1.2 * h;
        self.ff_sensed += (wf_dyn - self.ff_sensed) / 0.4 * h;
        self.fuel_used_kg += wf_dyn * h;

        // ---- Oil ---------------------------------------------------------------
        let o = &spec.oil;
        self.oil_qty_qt = (self.oil_qty_qt - f.oil_leak_qt_per_min / 60.0 * h).max(0.0);
        let mut oil_target = self.oil_temp_target(&inl);
        if f.fire && matches!(self.mode, TpMode::Running | TpMode::Starting) { oil_target += 60.0 }
        if self.oil_qty_qt < o.starvation_qt { oil_target += 30.0 * (1.0 - self.oil_qty_qt / o.starvation_qt) }
        self.oil_temp_k += (oil_target - self.oil_temp_k) / o.temp_tau_s * h;
        let mut press = o.press_gain_psi_per_pct * self.ng + o.press_offset_psi - 0.2 * (self.oil_temp_k - 273.15 - 80.0).max(0.0);
        if self.oil_qty_qt < o.starvation_qt { press *= self.oil_qty_qt / o.starvation_qt }
        self.oil_press_psi = if self.ng > 3.0 { press.max(0.0) + 0.4 * self.noise() } else { 0.0 };
        if matches!(self.mode, TpMode::Running | TpMode::Starting) && self.oil_press_psi < spec.limits.oil_press_min_idle_psi { self.oil_starved_s += h } else { self.oil_starved_s = (self.oil_starved_s - h).max(0.0) }
        if self.oil_starved_s > o.seizure_time_s && !self.seized { self.seized = true; self.lit = false }
        if f.fire_bottle { if c.condition_lever == 0 { self.faults.fire = false; self.bottle_msg_timer = 0.0 } else { self.bottle_msg_timer = 5.0 } }

        if self.mode == TpMode::Running { self.run_time_s += h }
        self.time_s += h;

        // ---- Warnings ------------------------------------------------------------
        let l = &spec.limits;
        let itt_c = self.itt_sensed_k - 273.15;
        if self.faults.fire { self.warnings.push("ENGINE FIRE".into()) }
        if self.seized { self.warnings.push("ENGINE SEIZED (oil starvation)".into()) }
        else if self.oil_starved_s > 5.0 { self.warnings.push(format!("OIL STARVATION {:.0} s", self.oil_starved_s)) }
        if f.chip_detector { self.warnings.push("CHIP DETECTOR: metal in oil, land as soon as practical".into()) }
        if f.itt_probe_fail { self.warnings.push("ITT INDICATION FAILED".into()) }
        if f.torque_sensor_fail { self.warnings.push("TORQUE INDICATION FAILED".into()) }
        if self.mode == TpMode::Starting && itt_c > l.itt_start_c { self.warnings.push(format!("HOT START: ITT {:.0} C exceeds start limit {:.0} C", itt_c, l.itt_start_c)) }
        else if self.mode == TpMode::Starting && itt_c > l.itt_max_c + 100.0 { self.warnings.push(format!("ITT {:.0} C rising fast during start", itt_c)) }
        if self.mode == TpMode::Running {
            if itt_c > l.itt_transient_c { self.warnings.push(format!("ITT {:.0} C exceeds {:.0} C", itt_c, l.itt_transient_c)) }
            else if itt_c > l.itt_max_c { self.warnings.push(format!("ITT {:.0} C above max continuous {:.0} C", itt_c, l.itt_max_c)) }
            if self.torque_nm > l.torque_max_nm { self.warnings.push(format!("TORQUE {:.0} ft lb exceeds {:.0} ft lb", self.torque_nm / FTLB, l.torque_max_nm / FTLB)) }
            if self.ng > l.ng_max_pct { self.warnings.push(format!("Ng {:.1}% exceeds {:.1}%", self.ng, l.ng_max_pct)) }
            if self.np > l.np_transient_rpm { self.warnings.push(format!("PROP OVERSPEED {:.0} rpm", self.np)) }
            else if self.np > l.np_max_rpm + 20.0 { self.warnings.push(format!("Np {:.0} rpm above {:.0}", self.np, l.np_max_rpm)) }
            let min_p = if self.ng < 65.0 { l.oil_press_min_idle_psi } else { l.oil_press_min_psi };
            if self.oil_press_psi < min_p { self.warnings.push(format!("LOW OIL PRESSURE {:.0} psi (min {:.0})", self.oil_press_psi, min_p)) }
            if self.oil_temp_k - 273.15 > l.oil_temp_max_c { self.warnings.push(format!("HIGH OIL TEMPERATURE {:.0} C", self.oil_temp_k - 273.15)) }
            if self.sm < 5.0 && self.surge_timer <= 0.0 { self.warnings.push(format!("Surge margin {:.1}%", self.sm)) }
            if f.prop_governor_fail { self.warnings.push("PROP GOVERNOR FAILED: blade angle frozen".into()) }
        }
        if self.oil_qty_qt < o.starvation_qt { self.warnings.push(format!("LOW OIL QUANTITY {:.1} qt", self.oil_qty_qt)) }
        if self.mode == TpMode::Starting && !self.lit && self.flameout_count > 0 { self.warnings.push("FLAMEOUT: auto-relight in progress".into()) }
        if matches!(self.mode, TpMode::Off | TpMode::Shutdown | TpMode::Motoring) && fuel_available && self.ng < sp.min_light_ng_pct { self.warnings.push(format!("NO LIGHT: Ng {:.0}% below {:.0}% (use starter)", self.ng, sp.min_light_ng_pct)) }
        if self.bottle_msg_timer > 0.0 { self.bottle_msg_timer -= h; self.warnings.push("FIRE BOTTLE DISCHARGED: fire persists, condition lever not at CUTOFF".into()) }

        self.last_cycle = r;
        if self.logger.due(self.time_s) {
            let state = self.state_inner(&inl);
            self.logger.maybe_sample(self.time_s, || tp_log_row(&state));
        }
    }

    pub fn state(&self) -> TpState {
        let inl = self.inlet();
        self.state_inner(&inl)
    }

    fn state_inner(&self, inl: &Inlet) -> TpState {
        let r = &self.last_cycle;
        let st = inl.theta.sqrt();
        let hp = self.shaft_power_w / HP;
        let pph = self.ff_sensed * 7936.64;
        let o = &self.spec.oil;
        let mut seed = self.seed;
        let mut nz = || { seed ^= seed >> 12; seed ^= seed << 25; seed ^= seed >> 27; ((seed.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64) - 0.5 };
        let vib = if self.ng > 5.0 { (0.2 + 0.6 * (self.ng / 100.0).powi(2) + if self.faults.fod { 2.0 } else { 0.0 } + if self.surge_timer > 0.0 { 1.5 } else { 0.0 } + (self.oil_starved_s / o.seizure_time_s).clamp(0.0, 1.0) * 2.0 + 0.1 * nz()).clamp(0.0, 5.0) } else { 0.0 };
        TpState {
            kind: "turboprop".into(),
            time_s: self.time_s,
            mode: self.mode,
            controls: self.controls,
            environment: self.environment,
            faults: self.faults,
            inlet: *inl,
            ng_pct: self.ng,
            ng_rpm: self.ng / 100.0 * self.spec.rating.ng_100pct_rpm,
            ng_corrected_pct: self.ng / st,
            ng_command_pct: self.ng_cmd,
            ng_idle_pct: self.ng_idle,
            np_rpm: self.np,
            np_pct: self.np / self.spec.rating.np_100pct_rpm * 100.0,
            np_target_rpm: self.np_target(),
            torque_nm: if self.faults.torque_sensor_fail { 0.0 } else { self.torque_nm },
            torque_ftlb: if self.faults.torque_sensor_fail { 0.0 } else { self.torque_nm / FTLB },
            torque_pct: self.torque_nm / self.spec.limits.torque_max_nm * 100.0,
            torque_valid: !self.faults.torque_sensor_fail,
            itt_c: if self.faults.itt_probe_fail { 0.0 } else { self.itt_sensed_k - 273.15 },
            itt_valid: !self.faults.itt_probe_fail,
            itt_gas_c: self.itt_gas_k - 273.15,
            t4_k: self.t4,
            t4_steady_k: self.t4_ss,
            shaft_power_w: self.shaft_power_w,
            shaft_power_hp: hp,
            power_pct: self.shaft_power_w / self.spec.rating.shaft_power_w * 100.0,
            fuel_flow_kg_s: self.ff_sensed,
            fuel_flow_pph: pph,
            fuel_flow_steady_kg_s: self.ff_steady,
            fuel_command_kg_s: self.wf_cmd,
            fuel_accel_limit_kg_s: self.wf_max,
            fuel_decel_limit_kg_s: self.wf_min,
            wf_p3_ratio: if self.ps3_kpa > 1.0 { self.wf * 3600.0 / self.ps3_kpa } else { 0.0 },
            ps3_kpa: self.ps3_kpa,
            fuel_used_kg: self.fuel_used_kg,
            sfc_lb_shp_h: if hp > 20.0 { pph / hp } else { 0.0 },
            prop: self.prop,
            prop_beta_deg: self.beta,
            feathered: self.beta > 80.0,
            governing: self.governing,
            thrust_n: self.prop.thrust_n + r.jet_thrust_n,
            thrust_lbf: (self.prop.thrust_n + r.jet_thrust_n) / 4.448222,
            jet_thrust_n: r.jet_thrust_n,
            oil_pressure_psi: self.oil_press_psi,
            oil_temperature_c: self.oil_temp_k - 273.15,
            oil_quantity_qt: self.oil_qty_qt,
            vib,
            starter_engaged: self.starter_engaged,
            ignition: self.mode == TpMode::Starting || self.controls.ignition,
            lit: self.lit,
            surge_margin_pct: self.sm,
            surge: self.surge_timer > 0.0,
            surge_count: self.surge_count,
            flameout_count: self.flameout_count,
            fire_warning: self.faults.fire,
            seized: self.seized,
            cycle: r.clone(),
            run_time_s: self.run_time_s,
            warnings: self.warnings.clone(),
            logger_rows: self.logger.len(),
            logger_enabled: self.logger.enabled,
        }
    }
}

pub fn tp_log_row(s: &TpState) -> Vec<f64> {
    let mode = match s.mode { TpMode::Off => 0.0, TpMode::Motoring => 1.0, TpMode::Starting => 2.0, TpMode::Running => 3.0, TpMode::Shutdown => 4.0 };
    let c = &s.controls;
    let cy = &s.cycle;
    vec![
        s.time_s, mode, c.power_lever, c.prop_lever, c.condition_lever as f64, c.starter as u8 as f64, c.ignition as u8 as f64, c.bleed_air as u8 as f64,
        s.environment.altitude_m, s.environment.mach, s.environment.delta_isa_k, s.inlet.tt2_k, s.inlet.pt2_pa,
        s.ng_pct, s.ng_corrected_pct, s.ng_command_pct, s.np_rpm, s.np_target_rpm, s.torque_ftlb, s.torque_pct,
        s.itt_c, s.itt_gas_c, s.t4_k, s.t4_steady_k, s.shaft_power_hp, s.power_pct,
        s.fuel_flow_kg_s, s.fuel_flow_pph, s.fuel_command_kg_s, s.fuel_accel_limit_kg_s, s.fuel_decel_limit_kg_s, s.wf_p3_ratio, s.ps3_kpa, s.fuel_used_kg, s.sfc_lb_shp_h,
        s.prop_beta_deg, s.prop.efficiency, s.thrust_n, s.jet_thrust_n, s.oil_pressure_psi, s.oil_temperature_c, s.oil_quantity_qt, s.vib,
        s.surge_margin_pct, s.surge as u8 as f64, s.lit as u8 as f64, s.fire_warning as u8 as f64,
        cy.w2, cy.pr, cy.tt3, cy.pt3, cy.tt45, cy.pt45, cy.tt5, cy.thermal_efficiency,
    ]
}
