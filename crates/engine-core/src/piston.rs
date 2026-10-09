//! Horizontally opposed, air-cooled, carburetted piston engine with a
//! fixed-pitch propeller: Lycoming O-360-A4M (180 hp), as fitted to the
//! Piper PA-28-181 Archer and the Cessna 172 (O-360 conversions / 172S has
//! the fuel-injected IO-360-L2A, 180 hp).
//!
//! Sources: Lycoming O-360 Operator's Manual (limits, mixture guidance),
//! FAA TCDS E-286, PA-28-181 POH (fuel flow, rpm, temperatures).

use crate::atmosphere::{isa, Ambient};
use crate::logger::DataLogger;
use crate::propeller::{prop_forces, PropForces, PropSpec};
use crate::spec::interp;
use serde::{Deserialize, Serialize};

const R_AIR: f64 = 287.05;
const TAU: f64 = std::f64::consts::TAU;
const INHG: f64 = 3386.39; // Pa per inHg
const GPH: f64 = 0.72 * 3.78541 / 3600.0; // kg/s per US gal/h of avgas

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonSpec {
    pub name: String,
    pub manufacturer: String,
    pub application: String,
    pub rating: PistonRating,
    pub geometry: PistonGeometry,
    pub induction: Induction,
    pub combustion: Combustion,
    pub thermal: Thermal,
    pub oil: PistonOil,
    pub start: PistonStart,
    pub propeller: PropSpec,
    pub limits: PistonLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonRating {
    pub rated_power_w: f64,
    pub rated_rpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonGeometry {
    pub displacement_m3: f64,
    pub cylinders: u32,
    pub compression_ratio: f64,
    pub dry_mass_kg: f64,
    /// Rotating inertia of crank + accessories (kg m^2), propeller added separately
    pub engine_inertia_kg_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Induction {
    pub carburetted: bool,
    pub volumetric_efficiency: f64,
    /// Full-throttle manifold pressure loss at rated rpm (Pa)
    pub full_throttle_loss_pa: f64,
    /// Idle manifold pressure (Pa) at idle rpm with the throttle closed
    pub idle_map_pa: f64,
    pub idle_rpm: f64,
    /// Throttle effective area shaping: a = a_idle + (1 - a_idle) * throttle^exp
    pub throttle_exponent: f64,
    /// Carburettor heat: induction air temperature rise (K) and the extra restriction (fraction)
    pub carb_heat_rise_k: f64,
    pub carb_heat_restriction: f64,
    /// Carburettor icing: OAT band (C) where it occurs, build and melt rates (fraction/s)
    pub carb_ice_oat_min_c: f64,
    pub carb_ice_oat_max_c: f64,
    pub carb_ice_build_rate: f64,
    pub carb_ice_melt_rate: f64,
    pub carb_ice_min_humidity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combustion {
    /// Fuel-air ratio at full rich (sea level) and the mixture lever curve [lever, F/A multiplier]
    pub fa_full_rich: f64,
    pub mixture_curve: Vec<[f64; 2]>,
    pub fa_stoichiometric: f64,
    /// Below this F/A the engine misfires / quits (lean limit)
    pub fa_lean_limit: f64,
    pub fuel_lhv_j_kg: f64,
    /// Indicated thermal efficiency at stoichiometric and above
    pub indicated_efficiency: f64,
    /// Friction mean effective pressure: fmep = a + b * rpm (Pa)
    pub fmep_a_pa: f64,
    pub fmep_b_pa_per_rpm: f64,
    /// Peak EGT (C) at stoichiometric: base + slope * load
    pub egt_base_c: f64,
    pub egt_load_slope_c: f64,
    pub egt_rich_slope_c_per_fa: f64,
    pub egt_lean_slope_c_per_fa: f64,
    /// Fuel system: engine-driven pump pressure (psi) and electric pump increment
    pub fuel_pressure_psi: f64,
    pub boost_pump_psi: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thermal {
    /// CHT target (F) = OAT_F + base + load * slope - cooling * TAS_kt + mixture terms
    pub cht_base_f: f64,
    pub cht_load_slope_f: f64,
    pub cht_cooling_f_per_kt: f64,
    pub cht_rich_cooling_f_per_fa: f64,
    pub cht_lean_cooling_f_per_fa: f64,
    pub cht_tau_s: f64,
    /// Oil temperature target (F) = OAT_F + base + load * slope - cooling * TAS
    pub oil_temp_base_f: f64,
    pub oil_temp_load_slope_f: f64,
    pub oil_temp_cooling_f_per_kt: f64,
    pub oil_temp_tau_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonOil {
    pub sump_qt: f64,
    pub min_qt: f64,
    /// psi = a + b * rpm - c * (oil_temp_F - 180)+
    pub press_a_psi: f64,
    pub press_b_psi_per_rpm: f64,
    pub press_temp_derate_psi_per_f: f64,
    pub consumption_qt_per_h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonStart {
    /// Starter cranking speed (rpm) and torque time constant
    pub cranking_rpm: f64,
    pub starter_tau_s: f64,
    /// Minimum rpm for the engine to fire
    pub min_fire_rpm: f64,
    /// Primer shots needed for a cold start, and the over-prime threshold (induction fire risk)
    pub prime_shots_needed: u32,
    pub overprime_shots: u32,
    /// Seconds a prime charge lasts while cranking
    pub prime_duration_s: f64,
    /// CHT (F) above which no prime is needed (warm engine)
    pub warm_cht_f: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonLimits {
    pub rpm_max: f64,
    pub cht_max_f: f64,
    pub cht_cruise_max_f: f64,
    pub oil_temp_max_f: f64,
    pub oil_press_min_idle_psi: f64,
    pub oil_press_min_psi: f64,
    pub oil_press_max_psi: f64,
    pub fuel_press_min_psi: f64,
    pub fuel_press_max_psi: f64,
    /// Expected magneto check rpm drop (each) and maximum difference between mags
    pub mag_drop_rpm: f64,
    pub mag_drop_max_rpm: f64,
    pub mag_diff_max_rpm: f64,
}

impl PistonSpec {
    /// Lycoming O-360-A4M, 180 hp at 2700 rpm, fixed-pitch Sensenich 76" prop
    /// (Piper PA-28-181 Archer).
    pub fn lycoming_o360() -> Self {
        PistonSpec {
            name: "O-360-A4M".into(),
            manufacturer: "Lycoming".into(),
            application: "Piper PA-28-181 Archer / Cessna 172 (180 hp)".into(),
            rating: PistonRating { rated_power_w: 134_226.0, rated_rpm: 2_700.0 },
            geometry: PistonGeometry { displacement_m3: 5.916e-3, cylinders: 4, compression_ratio: 8.5, dry_mass_kg: 118.0, engine_inertia_kg_m2: 0.12 },
            induction: Induction {
                carburetted: true,
                volumetric_efficiency: 0.85,
                full_throttle_loss_pa: 2_400.0,
                idle_map_pa: 10.0 * INHG,
                idle_rpm: 650.0,
                throttle_exponent: 1.8,
                carb_heat_rise_k: 35.0,
                carb_heat_restriction: 0.04,
                carb_ice_oat_min_c: -7.0,
                carb_ice_oat_max_c: 21.0,
                carb_ice_build_rate: 0.004,
                carb_ice_melt_rate: 0.03,
                carb_ice_min_humidity: 0.55,
            },
            combustion: Combustion {
                fa_full_rich: 0.088,
                mixture_curve: vec![[0.0, 0.0], [0.08, 0.0], [0.12, 0.55], [0.3, 0.68], [0.5, 0.78], [0.7, 0.88], [0.85, 0.95], [1.0, 1.0]],
                fa_stoichiometric: 0.0667,
                fa_lean_limit: 0.052,
                fuel_lhv_j_kg: 43.5e6,
                indicated_efficiency: 0.40,
                fmep_a_pa: 60_000.0,
                fmep_b_pa_per_rpm: 35.0,
                egt_base_c: 560.0,
                egt_load_slope_c: 300.0,
                egt_rich_slope_c_per_fa: 7_000.0,
                egt_lean_slope_c_per_fa: 9_000.0,
                fuel_pressure_psi: 4.5,
                boost_pump_psi: 1.5,
            },
            thermal: Thermal {
                cht_base_f: 230.0,
                cht_load_slope_f: 200.0,
                cht_cooling_f_per_kt: 0.7,
                cht_rich_cooling_f_per_fa: 3_000.0,
                cht_lean_cooling_f_per_fa: 2_500.0,
                cht_tau_s: 90.0,
                oil_temp_base_f: 100.0,
                oil_temp_load_slope_f: 70.0,
                oil_temp_cooling_f_per_kt: 0.3,
                oil_temp_tau_s: 240.0,
            },
            oil: PistonOil { sump_qt: 8.0, min_qt: 2.0, press_a_psi: 10.0, press_b_psi_per_rpm: 0.028, press_temp_derate_psi_per_f: 0.15, consumption_qt_per_h: 0.1 },
            start: PistonStart { cranking_rpm: 170.0, starter_tau_s: 0.8, min_fire_rpm: 90.0, prime_shots_needed: 2, overprime_shots: 6, prime_duration_s: 8.0, warm_cht_f: 200.0 },
            propeller: PropSpec {
                diameter_m: 1.93, // 76 in
                blades: 2,
                blade_area_m2: 0.130,
                fixed_pitch_deg: Some(22.0),
                beta_min_deg: 22.0,
                beta_max_deg: 22.0,
                beta_feather_deg: 22.0,
                beta_reverse_deg: 22.0,
                cl_alpha_per_rad: 5.5,
                cl_max: 1.35,
                alpha_zero_lift_deg: -2.0,
                alpha_stall_deg: 14.0,
                alpha_stall_neg_deg: 10.0,
                cl_max_neg: 0.9,
                cd0: 0.012,
                k_induced: 0.05,
                inertia_kg_m2: 0.30,
            },
            limits: PistonLimits {
                rpm_max: 2_700.0,
                cht_max_f: 500.0,
                cht_cruise_max_f: 435.0,
                oil_temp_max_f: 245.0,
                oil_press_min_idle_psi: 25.0,
                oil_press_min_psi: 55.0,
                oil_press_max_psi: 95.0,
                fuel_press_min_psi: 0.5,
                fuel_press_max_psi: 8.0,
                mag_drop_rpm: 100.0,
                mag_drop_max_rpm: 175.0,
                mag_diff_max_rpm: 50.0,
            },
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("spec serialises")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PistonControls {
    pub throttle: f64,
    pub mixture: f64,
    /// 0 OFF, 1 LEFT, 2 RIGHT, 3 BOTH
    pub magnetos: u8,
    pub starter: bool,
    pub carb_heat: bool,
    /// Primer: number of shots pumped (decays while cranking)
    pub primer_shots: u32,
    pub fuel_pump: bool,
    /// 0 OFF, 1 LEFT, 2 RIGHT, 3 BOTH
    pub fuel_selector: u8,
    /// Fuel quantity per tank (US gal)
    pub fuel_left_gal: f64,
    pub fuel_right_gal: f64,
}

impl Default for PistonControls {
    fn default() -> Self {
        PistonControls { throttle: 0.0, mixture: 1.0, magnetos: 0, starter: false, carb_heat: false, primer_shots: 0, fuel_pump: false, fuel_selector: 3, fuel_left_gal: 24.0, fuel_right_gal: 24.0 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PistonEnvironment {
    pub altitude_m: f64,
    /// True airspeed (m/s)
    pub tas_m_s: f64,
    pub delta_isa_k: f64,
    /// Relative humidity 0..1 (carburettor icing)
    pub humidity: f64,
}

impl Default for PistonEnvironment {
    fn default() -> Self {
        PistonEnvironment { altitude_m: 0.0, tas_m_s: 0.0, delta_isa_k: 0.0, humidity: 0.4 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PistonFaults {
    pub mag_left_fail: bool,
    pub mag_right_fail: bool,
    /// One spark plug fouled (left magneto side of cylinder 3): rough on LEFT alone
    pub plug_fouled: bool,
    /// One cylinder dead (stuck valve): 25 % power loss, rough
    pub cylinder_dead: bool,
    pub oil_leak_qt_per_min: f64,
    /// Vapour lock: hot engine will not fire until the boost pump has run
    pub vapor_lock: bool,
    /// Induction fire after over-priming (started automatically, or injected)
    pub induction_fire: bool,
    /// Carburettor ice can form (otherwise dry air)
    pub carb_ice_enabled: bool,
    /// Starter inoperative
    pub starter_inop: bool,
}

impl PistonFaults {
    pub fn none() -> Self {
        PistonFaults { carb_ice_enabled: true, ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PistonMode {
    Off,
    Cranking,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonState {
    pub kind: String,
    pub time_s: f64,
    pub mode: PistonMode,
    pub controls: PistonControls,
    pub environment: PistonEnvironment,
    pub faults: PistonFaults,
    pub ambient: Ambient,
    pub rpm: f64,
    pub map_inhg: f64,
    pub map_pa: f64,
    pub egt_f: f64,
    pub egt_c: f64,
    pub cht_f: f64,
    pub cht_c: f64,
    pub oil_pressure_psi: f64,
    pub oil_temperature_f: f64,
    pub oil_temperature_c: f64,
    pub oil_quantity_qt: f64,
    pub fuel_flow_gph: f64,
    pub fuel_flow_kg_s: f64,
    pub fuel_pressure_psi: f64,
    pub fuel_used_gal: f64,
    pub fuel_air_ratio: f64,
    pub air_flow_kg_s: f64,
    pub power_w: f64,
    pub power_hp: f64,
    pub power_pct: f64,
    pub indicated_power_w: f64,
    pub friction_power_w: f64,
    pub torque_nm: f64,
    pub bsfc_lb_hp_h: f64,
    pub thermal_efficiency: f64,
    pub prop: PropForces,
    pub thrust_n: f64,
    pub thrust_lbf: f64,
    pub carb_ice: f64,
    pub induction_temp_c: f64,
    pub firing: bool,
    pub roughness: f64,
    pub cylinders_firing: f64,
    pub primed: bool,
    pub run_time_s: f64,
    pub warnings: Vec<String>,
    pub logger_rows: usize,
    pub logger_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PistonEngine {
    pub spec: PistonSpec,
    pub controls: PistonControls,
    pub environment: PistonEnvironment,
    pub faults: PistonFaults,
    pub logger: DataLogger,
    time_s: f64,
    mode: PistonMode,
    rpm: f64,
    firing: bool,
    prime_timer: f64,
    cranking_since: f64,
    pump_run_s: f64,
    map_pa: f64,
    egt_gas_c: f64,
    egt_sensed_c: f64,
    cht_f: f64,
    oil_temp_f: f64,
    oil_press_psi: f64,
    oil_qty_qt: f64,
    fuel_flow_kg_s: f64,
    fuel_used_gal: f64,
    fuel_starve_s: f64,
    carb_ice: f64,
    power_w: f64,
    ind_power_w: f64,
    fric_power_w: f64,
    fa: f64,
    air_flow: f64,
    prop: PropForces,
    roughness: f64,
    cyl_firing: f64,
    run_time_s: f64,
    seed: u64,
    warnings: Vec<String>,
    max_substep_s: f64,
}

pub const PISTON_LOG_COLUMNS: &[&str] = &[
    "time_s", "mode", "throttle", "mixture", "magnetos", "starter", "carb_heat", "fuel_pump", "fuel_selector",
    "altitude_m", "tas_m_s", "delta_isa_k", "humidity",
    "rpm", "map_inhg", "egt_f", "cht_f", "oil_pressure_psi", "oil_temperature_f", "oil_quantity_qt",
    "fuel_flow_gph", "fuel_pressure_psi", "fuel_used_gal", "fuel_air_ratio", "air_flow_kg_s",
    "power_hp", "power_pct", "torque_nm", "bsfc_lb_hp_h", "thrust_n", "prop_efficiency", "carb_ice", "roughness", "firing",
];

impl PistonEngine {
    pub fn new(spec: PistonSpec) -> Self {
        let amb = isa(0.0, 0.0);
        let oat_f = (amb.temperature_k - 273.15) * 1.8 + 32.0;
        let columns = PISTON_LOG_COLUMNS.iter().map(|s| s.to_string()).collect();
        let sump = spec.oil.sump_qt;
        PistonEngine {
            spec,
            controls: PistonControls::default(),
            environment: PistonEnvironment::default(),
            faults: PistonFaults::none(),
            logger: DataLogger::new(10.0, columns),
            time_s: 0.0,
            mode: PistonMode::Off,
            rpm: 0.0,
            firing: false,
            prime_timer: 0.0,
            cranking_since: 0.0,
            pump_run_s: 0.0,
            map_pa: amb.pressure_pa,
            egt_gas_c: amb.temperature_k - 273.15,
            egt_sensed_c: amb.temperature_k - 273.15,
            cht_f: oat_f,
            oil_temp_f: oat_f,
            oil_press_psi: 0.0,
            oil_qty_qt: sump,
            fuel_flow_kg_s: 0.0,
            fuel_used_gal: 0.0,
            fuel_starve_s: 0.0,
            carb_ice: 0.0,
            power_w: 0.0,
            ind_power_w: 0.0,
            fric_power_w: 0.0,
            fa: 0.0,
            air_flow: 0.0,
            prop: PropForces::default(),
            roughness: 0.0,
            cyl_firing: 0.0,
            run_time_s: 0.0,
            seed: 0x1234_5678_9ABC_DEF1,
            warnings: Vec::new(),
            max_substep_s: 0.01,
        }
    }

    pub fn reset(&mut self) {
        let spec = self.spec.clone();
        let (c, e, f, rate) = (self.controls, self.environment, self.faults, self.logger.rate_hz);
        *self = PistonEngine::new(spec);
        self.controls = c;
        self.environment = e;
        self.faults = f;
        self.logger.set_rate(rate);
    }

    pub fn time(&self) -> f64 {
        self.time_s
    }

    /// Jump to a warm, running condition at the given throttle (cruise / run-up).
    pub fn set_running(&mut self, throttle: f64) {
        self.controls.throttle = throttle.clamp(0.0, 1.0);
        self.controls.magnetos = 3;
        self.controls.starter = false;
        self.controls.mixture = self.controls.mixture.max(0.85);
        self.mode = PistonMode::Running;
        self.firing = true;
        self.rpm = 650.0 + 2_000.0 * self.controls.throttle;
        self.cht_f = 330.0;
        self.oil_temp_f = 180.0;
        self.egt_gas_c = 700.0;
        self.egt_sensed_c = 700.0;
        // Settle the rpm quickly
        for _ in 0..600 {
            self.substep(0.02);
        }
        self.time_s = 0.0;
        self.run_time_s = 0.0;
        self.fuel_used_gal = 0.0;
        self.logger.clear();
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
        let amb = isa(self.environment.altitude_m, self.environment.delta_isa_k);
        let tas = self.environment.tas_m_s.max(0.0);
        let tas_kt = tas * 1.94384;
        let oat_c = amb.temperature_k - 273.15;
        let oat_f = oat_c * 1.8 + 32.0;
        self.warnings.clear();

        // ---- Fuel availability ---------------------------------------------
        let tank_ok = match c.fuel_selector {
            1 => c.fuel_left_gal > 0.05,
            2 => c.fuel_right_gal > 0.05,
            3 => c.fuel_left_gal > 0.05 || c.fuel_right_gal > 0.05,
            _ => false,
        };
        if tank_ok { self.fuel_starve_s = 0.0 } else { self.fuel_starve_s += h }
        let fuel_available = self.fuel_starve_s < 6.0; // float bowl
        if c.fuel_pump { self.pump_run_s += h } else { self.pump_run_s = 0.0 }

        // ---- Induction: carburettor heat and icing ---------------------------
        let ind = &spec.induction;
        let mut t_ind = amb.temperature_k;
        let mut restriction = 1.0;
        if c.carb_heat {
            t_ind += ind.carb_heat_rise_k;
            restriction *= 1.0 - ind.carb_heat_restriction;
        }
        if f.carb_ice_enabled {
            let icing_conditions = oat_c > ind.carb_ice_oat_min_c && oat_c < ind.carb_ice_oat_max_c && self.environment.humidity >= ind.carb_ice_min_humidity && !c.carb_heat && self.mode == PistonMode::Running;
            if icing_conditions {
                // Partial throttle (venturi + throttle plate cooling) ices fastest
                let factor = 1.0 - 0.6 * c.throttle;
                self.carb_ice = (self.carb_ice + ind.carb_ice_build_rate * factor * h).min(0.85);
            } else if c.carb_heat || oat_c > ind.carb_ice_oat_max_c + 5.0 {
                self.carb_ice = (self.carb_ice - ind.carb_ice_melt_rate * h).max(0.0);
            }
        } else {
            self.carb_ice = (self.carb_ice - ind.carb_ice_melt_rate * h).max(0.0);
        }
        restriction *= 1.0 - 0.75 * self.carb_ice;

        // ---- Manifold pressure and air flow -----------------------------------
        // Throttle effective area; the pressure drop across it scales with the
        // square of the air flow, and the air flow with MAP (speed-density).
        let a_idle = {
            // Derived so that idle rpm with the throttle closed gives the idle MAP
            let c_full = ind.volumetric_efficiency * spec.geometry.displacement_m3 * spec.rating.rated_rpm / (120.0 * R_AIR * 288.15);
            let m_full = c_full * (101_325.0 - ind.full_throttle_loss_pa);
            let x_full = ind.full_throttle_loss_pa / (m_full * m_full);
            let c_idle = ind.volumetric_efficiency * spec.geometry.displacement_m3 * ind.idle_rpm / (120.0 * R_AIR * 288.15);
            let m_idle = c_idle * ind.idle_map_pa;
            let x_idle = (101_325.0 - ind.idle_map_pa) / (m_idle * m_idle);
            (x_full / x_idle).sqrt()
        };
        let c_full = ind.volumetric_efficiency * spec.geometry.displacement_m3 * spec.rating.rated_rpm / (120.0 * R_AIR * 288.15);
        let m_full = c_full * (101_325.0 - ind.full_throttle_loss_pa);
        let x_full = ind.full_throttle_loss_pa / (m_full * m_full);
        let a_eff = (a_idle + (1.0 - a_idle) * c.throttle.clamp(0.0, 1.0).powf(ind.throttle_exponent)) * restriction;
        let x = x_full / (a_eff * a_eff);
        let p = amb.pressure_pa;
        // Volumetric efficiency falls at low manifold pressure (residual exhaust
        // gas re-expands and fills part of the cylinder): uses last step's MAP.
        let map_ratio_prev = (self.map_pa / p).clamp(0.05, 1.0);
        let eta_v = ind.volumetric_efficiency * (0.45 + 0.55 * map_ratio_prev);
        let cc = eta_v * spec.geometry.displacement_m3 * self.rpm.max(1.0) / (120.0 * R_AIR * t_ind);
        // x (cc MAP)^2 + MAP - p = 0
        let k = x * cc * cc;
        self.map_pa = if k > 1e-30 { (-1.0 + (1.0 + 4.0 * k * p).sqrt()) / (2.0 * k) } else { p };
        self.air_flow = cc * self.map_pa;
        let map_ratio = (self.map_pa / p).clamp(0.05, 1.0);

        // ---- Mixture: carburettor fuel metering ---------------------------------
        let cb = &spec.combustion;
        // The venturi sits upstream of the throttle, at ambient pressure and
        // the induction air temperature. A fixed jet meters fuel by venturi
        // pressure drop: F/A rises as the air gets thinner (hence leaning with
        // altitude, and carb heat enriching).
        let rho_carb = p / (R_AIR * t_ind);
        let rho0 = 101_325.0 / (R_AIR * 288.15);
        let altitude_enrichment = (rho0 / rho_carb.max(0.2)).sqrt().min(1.6);
        let mut fa = cb.fa_full_rich * interp(&cb.mixture_curve, c.mixture.clamp(0.0, 1.0)) * altitude_enrichment;
        if !fuel_available || f.vapor_lock && self.cht_f > spec.start.warm_cht_f && self.pump_run_s < 20.0 && self.mode != PistonMode::Running {
            fa = 0.0;
        }
        self.fa = fa;

        // ---- Ignition and firing ------------------------------------------------
        let mag_l = (c.magnetos == 1 || c.magnetos == 3) && !f.mag_left_fail;
        let mag_r = (c.magnetos == 2 || c.magnetos == 3) && !f.mag_right_fail;
        let ignition = mag_l || mag_r;
        let st = &spec.start;
        let warm = self.cht_f > st.warm_cht_f;
        if c.primer_shots > 0 && self.prime_timer <= 0.0 {
            self.prime_timer = st.prime_duration_s;
        }
        let primed = self.prime_timer > 0.0 || warm;
        if self.prime_timer > 0.0 { self.prime_timer -= h }
        if c.primer_shots >= st.overprime_shots && self.mode == PistonMode::Cranking && !self.faults.induction_fire {
            self.faults.induction_fire = true;
        }
        let can_fire = ignition && fa >= cb.fa_lean_limit && self.rpm >= st.min_fire_rpm && (primed || self.rpm > 400.0);
        if can_fire {
            if !self.firing {
                self.firing = true;
                self.controls.primer_shots = 0;
            }
        } else {
            self.firing = false;
        }

        // Cylinders actually producing power: single-mag running loses a little
        // (slower flame), a fouled plug loses one cylinder on that mag, a dead
        // cylinder is gone on both.
        let mut cyl = spec.geometry.cylinders as f64;
        let mut single_mag_loss = 0.0;
        if self.firing {
            if !(mag_l && mag_r) {
                single_mag_loss = 0.07;
            }
            if f.plug_fouled && mag_l && !mag_r {
                cyl -= 1.0;
            }
            if f.cylinder_dead {
                cyl -= 1.0;
            }
        }
        self.cyl_firing = if self.firing { cyl } else { 0.0 };

        // ---- Power ----------------------------------------------------------------
        let fa_burn = fa.min(cb.fa_stoichiometric);
        let lean_factor = if fa >= cb.fa_lean_limit { ((fa - cb.fa_lean_limit) / (0.062 - cb.fa_lean_limit)).clamp(0.0, 1.0) } else { 0.0 };
        let rich_bonus = 1.0 + 0.5 * (fa - cb.fa_stoichiometric).clamp(0.0, 0.012);
        // Indicated efficiency falls at part load (slower burn with residual gas)
        let eta_i = cb.indicated_efficiency * (0.55 + 0.45 * map_ratio);
        let ind_power = if self.firing {
            self.air_flow * fa_burn * cb.fuel_lhv_j_kg * eta_i * lean_factor * rich_bonus * (cyl / spec.geometry.cylinders as f64) * (1.0 - single_mag_loss)
        } else {
            0.0
        };
        // Friction plus pumping work across the closed throttle
        let fmep = cb.fmep_a_pa + cb.fmep_b_pa_per_rpm * self.rpm + (p - self.map_pa).max(0.0);
        let fric_power = fmep * spec.geometry.displacement_m3 * self.rpm / 120.0;
        self.ind_power_w = ind_power;
        self.fric_power_w = fric_power;
        self.power_w = ind_power - fric_power;
        let omega = self.rpm / 60.0 * TAU;
        let q_engine = if omega > 1.0 { self.power_w / omega } else { 0.0 };
        self.fuel_flow_kg_s = if self.firing || (self.mode == PistonMode::Cranking && fa > 0.0) { self.air_flow * fa } else { 0.0 };

        // ---- Propeller and rpm dynamics ----------------------------------------
        let pr = &spec.propeller;
        let beta = pr.fixed_pitch_deg.unwrap_or(pr.beta_min_deg);
        self.prop = prop_forces(pr, amb.density_kg_m3, amb.speed_of_sound_m_s, tas, self.rpm, beta);
        let j_total = pr.inertia_kg_m2 + spec.geometry.engine_inertia_kg_m2;
        let starter_ok = c.starter && !f.starter_inop;
        let q_starter = if starter_ok && self.rpm < st.cranking_rpm * 1.8 {
            // Starter motor: stall torque sized to crank the cold engine at the
            // cranking speed against friction and pumping; torque falls
            // linearly to zero at twice that speed.
            let omega_crank = st.cranking_rpm / 60.0 * TAU;
            let fmep_crank = cb.fmep_a_pa + cb.fmep_b_pa_per_rpm * st.cranking_rpm + 0.6 * p;
            let q_fric_crank = fmep_crank * spec.geometry.displacement_m3 / (2.0 * TAU);
            let q_stall = 2.0 * q_fric_crank * (1.0 + 0.3 * (120.0 - self.oil_temp_f).max(0.0) / 120.0);
            q_stall * (1.0 - omega / (2.0 * omega_crank)).max(0.0)
        } else {
            0.0
        };
        let q_prop = self.prop.torque_nm * (self.rpm.signum().max(0.0));
        let mut domega = (q_engine + q_starter - q_prop) / j_total;
        // Windmilling: airflow can turn a stopped prop at high airspeed
        if !self.firing && tas > 30.0 && self.rpm < 300.0 {
            domega += 0.5 * (tas - 30.0);
        }
        self.rpm = (self.rpm + domega * h / TAU * 60.0).max(0.0);
        if !self.firing && q_starter <= 0.0 && self.rpm < 15.0 {
            self.rpm = 0.0;
        }

        // ---- Mode -----------------------------------------------------------------
        self.mode = if self.firing && self.rpm > 300.0 {
            PistonMode::Running
        } else if starter_ok {
            if self.mode != PistonMode::Cranking { self.cranking_since = self.time_s }
            PistonMode::Cranking
        } else {
            PistonMode::Off
        };
        if self.mode == PistonMode::Running { self.run_time_s += h }
        if self.mode == PistonMode::Cranking && c.primer_shots == 0 && !warm && self.time_s - self.cranking_since > 6.0 && !self.firing {
            self.warnings.push("NO START: cold engine, not primed".into());
        }
        if self.mode == PistonMode::Cranking && !ignition && self.time_s - self.cranking_since > 3.0 {
            self.warnings.push("NO START: magnetos OFF".into());
        }

        // ---- Fuel accounting -------------------------------------------------
        let gal = self.fuel_flow_kg_s * h / (0.72 * 3.78541);
        self.fuel_used_gal += gal;
        match c.fuel_selector {
            1 => self.controls.fuel_left_gal = (self.controls.fuel_left_gal - gal).max(0.0),
            2 => self.controls.fuel_right_gal = (self.controls.fuel_right_gal - gal).max(0.0),
            3 => {
                let half = gal / 2.0;
                self.controls.fuel_left_gal = (self.controls.fuel_left_gal - half).max(0.0);
                self.controls.fuel_right_gal = (self.controls.fuel_right_gal - half).max(0.0);
            }
            _ => {}
        }

        // ---- Temperatures ------------------------------------------------------
        let load = (self.power_w / spec.rating.rated_power_w).clamp(0.0, 1.2);
        let th = &spec.thermal;
        let egt_target = if self.firing {
            let d = fa - cb.fa_stoichiometric;
            let slope = if d > 0.0 { cb.egt_rich_slope_c_per_fa } else { cb.egt_lean_slope_c_per_fa };
            cb.egt_base_c + cb.egt_load_slope_c * load - slope * d.abs()
        } else {
            oat_c + 0.3 * (self.egt_gas_c - oat_c)
        };
        self.egt_gas_c += (egt_target - self.egt_gas_c) / (if self.firing { 2.0 } else { 40.0 }) * h;
        self.egt_sensed_c += (self.egt_gas_c - self.egt_sensed_c) / 3.0 * h;
        let cht_target = if self.firing {
            let d = fa - 0.075;
            let mix = if d > 0.0 { -th.cht_rich_cooling_f_per_fa * d } else { -th.cht_lean_cooling_f_per_fa * (-d) };
            oat_f + th.cht_base_f + th.cht_load_slope_f * load - th.cht_cooling_f_per_kt * tas_kt + mix
        } else {
            oat_f + 20.0
        };
        let cht_tau = if self.firing { th.cht_tau_s } else { th.cht_tau_s * 3.0 };
        self.cht_f += (cht_target - self.cht_f) / cht_tau * h;
        let oil_target = if self.firing { oat_f + th.oil_temp_base_f + th.oil_temp_load_slope_f * load - th.oil_temp_cooling_f_per_kt * tas_kt } else { oat_f + 10.0 };
        if self.faults.induction_fire { self.cht_f += 2.0 * h }
        self.oil_temp_f += (oil_target - self.oil_temp_f) / (if self.firing { th.oil_temp_tau_s } else { th.oil_temp_tau_s * 2.0 }) * h;

        // ---- Oil ------------------------------------------------------------------
        let o = &spec.oil;
        self.oil_qty_qt = (self.oil_qty_qt - f.oil_leak_qt_per_min / 60.0 * h - if self.firing { o.consumption_qt_per_h / 3600.0 * h } else { 0.0 }).max(0.0);
        let mut press = o.press_a_psi + o.press_b_psi_per_rpm * self.rpm - o.press_temp_derate_psi_per_f * (self.oil_temp_f - 180.0).max(0.0);
        // Cold oil: much higher pressure (relief valve limits it in the real engine)
        press += 0.6 * (120.0 - self.oil_temp_f).max(0.0);
        press = press.min(115.0);
        if self.oil_qty_qt < o.min_qt {
            press *= self.oil_qty_qt / o.min_qt;
        }
        self.oil_press_psi = if self.rpm > 50.0 { press.max(0.0) + 0.4 * self.noise() } else { 0.0 };

        // ---- Roughness --------------------------------------------------------
        let mut rough = 0.0;
        if self.firing {
            if cyl < spec.geometry.cylinders as f64 { rough += 0.6 }
            if fa < 0.058 { rough += 0.3 }
            if self.carb_ice > 0.3 { rough += 0.3 * self.carb_ice }
            if c.carb_heat && self.carb_ice > 0.1 { rough += 0.4 } // ice melting through the engine
        }
        self.roughness += (rough - self.roughness) / 0.5 * h;

        // ---- Warnings ------------------------------------------------------------
        let l = &spec.limits;
        if self.faults.induction_fire {
            self.warnings.push("INDUCTION FIRE (over-primed): keep cranking, mixture cutoff".into());
        }
        if self.mode == PistonMode::Running {
            if self.rpm > l.rpm_max + 10.0 { self.warnings.push(format!("RPM {:.0} exceeds {:.0}", self.rpm, l.rpm_max)) }
            if self.cht_f > l.cht_max_f { self.warnings.push(format!("CHT {:.0} F exceeds {:.0} F", self.cht_f, l.cht_max_f)) }
            else if self.cht_f > l.cht_cruise_max_f { self.warnings.push(format!("CHT {:.0} F above cruise limit {:.0} F", self.cht_f, l.cht_cruise_max_f)) }
            if self.oil_temp_f > l.oil_temp_max_f { self.warnings.push(format!("OIL TEMP {:.0} F exceeds {:.0} F", self.oil_temp_f, l.oil_temp_max_f)) }
            let min_p = if self.rpm < 1_200.0 { l.oil_press_min_idle_psi } else { l.oil_press_min_psi };
            if self.oil_press_psi < min_p { self.warnings.push(format!("OIL PRESSURE {:.0} psi below {:.0} psi", self.oil_press_psi, min_p)) }
            if self.oil_press_psi > l.oil_press_max_psi + 20.0 { self.warnings.push(format!("OIL PRESSURE {:.0} psi high (cold oil)", self.oil_press_psi)) }
            if self.carb_ice > 0.15 { self.warnings.push(format!("CARB ICE forming ({:.0}%): MAP / rpm dropping, apply carb heat", self.carb_ice * 100.0)) }
            if !fuel_available { self.warnings.push("FUEL STARVATION".into()) }
            if self.roughness > 0.4 { self.warnings.push("ENGINE ROUGH".into()) }
        }
        if self.oil_qty_qt < o.min_qt { self.warnings.push(format!("OIL QUANTITY {:.1} qt below minimum", self.oil_qty_qt)) }
        self.time_s += h;

        if self.logger.due(self.time_s) {
            let state = self.state_inner(&amb);
            self.logger.maybe_sample(self.time_s, || piston_log_row(&state));
        }
    }

    pub fn state(&self) -> PistonState {
        let amb = isa(self.environment.altitude_m, self.environment.delta_isa_k);
        self.state_inner(&amb)
    }

    fn state_inner(&self, amb: &Ambient) -> PistonState {
        let hp = self.power_w / 745.7;
        let fuel_pph = self.fuel_flow_kg_s * 7936.64;
        let cb = &self.spec.combustion;
        let fuel_press = if self.rpm > 50.0 || self.controls.fuel_pump {
            (if self.rpm > 50.0 { cb.fuel_pressure_psi } else { 0.0 }) + (if self.controls.fuel_pump { cb.boost_pump_psi } else { 0.0 })
        } else { 0.0 };
        PistonState {
            kind: "piston".into(),
            time_s: self.time_s,
            mode: self.mode,
            controls: self.controls,
            environment: self.environment,
            faults: self.faults,
            ambient: *amb,
            rpm: self.rpm,
            map_inhg: self.map_pa / INHG,
            map_pa: self.map_pa,
            egt_f: self.egt_sensed_c * 1.8 + 32.0,
            egt_c: self.egt_sensed_c,
            cht_f: self.cht_f,
            cht_c: (self.cht_f - 32.0) / 1.8,
            oil_pressure_psi: self.oil_press_psi,
            oil_temperature_f: self.oil_temp_f,
            oil_temperature_c: (self.oil_temp_f - 32.0) / 1.8,
            oil_quantity_qt: self.oil_qty_qt,
            fuel_flow_gph: self.fuel_flow_kg_s / GPH,
            fuel_flow_kg_s: self.fuel_flow_kg_s,
            fuel_pressure_psi: fuel_press,
            fuel_used_gal: self.fuel_used_gal,
            fuel_air_ratio: self.fa,
            air_flow_kg_s: self.air_flow,
            power_w: self.power_w,
            power_hp: hp,
            power_pct: self.power_w / self.spec.rating.rated_power_w * 100.0,
            indicated_power_w: self.ind_power_w,
            friction_power_w: self.fric_power_w,
            torque_nm: if self.rpm > 1.0 { self.power_w / (self.rpm / 60.0 * TAU) } else { 0.0 },
            bsfc_lb_hp_h: if hp > 5.0 { fuel_pph / hp } else { 0.0 },
            thermal_efficiency: if self.fuel_flow_kg_s > 1e-6 { (self.power_w / (self.fuel_flow_kg_s * cb.fuel_lhv_j_kg)).clamp(0.0, 1.0) } else { 0.0 },
            prop: self.prop,
            thrust_n: self.prop.thrust_n,
            thrust_lbf: self.prop.thrust_n / 4.448222,
            carb_ice: self.carb_ice,
            induction_temp_c: amb.temperature_k - 273.15 + if self.controls.carb_heat { self.spec.induction.carb_heat_rise_k } else { 0.0 },
            firing: self.firing,
            roughness: self.roughness,
            cylinders_firing: self.cyl_firing,
            primed: self.prime_timer > 0.0,
            run_time_s: self.run_time_s,
            warnings: self.warnings.clone(),
            logger_rows: self.logger.len(),
            logger_enabled: self.logger.enabled,
        }
    }
}

pub fn piston_log_row(s: &PistonState) -> Vec<f64> {
    let mode = match s.mode { PistonMode::Off => 0.0, PistonMode::Cranking => 1.0, PistonMode::Running => 2.0 };
    let c = &s.controls;
    vec![
        s.time_s, mode, c.throttle, c.mixture, c.magnetos as f64, c.starter as u8 as f64, c.carb_heat as u8 as f64, c.fuel_pump as u8 as f64, c.fuel_selector as f64,
        s.environment.altitude_m, s.environment.tas_m_s, s.environment.delta_isa_k, s.environment.humidity,
        s.rpm, s.map_inhg, s.egt_f, s.cht_f, s.oil_pressure_psi, s.oil_temperature_f, s.oil_quantity_qt,
        s.fuel_flow_gph, s.fuel_pressure_psi, s.fuel_used_gal, s.fuel_air_ratio, s.air_flow_kg_s,
        s.power_hp, s.power_pct, s.torque_nm, s.bsfc_lb_hp_h, s.thrust_n, s.prop.efficiency, s.carb_ice, s.roughness, s.firing as u8 as f64,
    ]
}
