//! Engine specification: every number the simulator needs, expressed in the
//! same terms the manufacturer and the aircraft documentation use.
//!
//! The default is the CFM International CFM56-7B26 as fitted to the Boeing
//! 737-800. All published figures (ratings, speeds, limits) come from the
//! FAA Type Certificate Data Sheet E00055EN, the Boeing 737NG FCOM limitations
//! chapter, and CFM published data. Cycle parameters that are not published
//! (component efficiencies, cooling flows) are typical values for an engine
//! of this generation and are the calibration knobs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSpec {
    pub name: String,
    pub manufacturer: String,
    pub application: String,
    pub kind: EngineKind,
    pub rating: Rating,
    pub geometry: Geometry,
    pub design_point: DesignPoint,
    pub spools: Spools,
    pub fadec: FadecSpec,
    pub bleed: BleedSpec,
    pub reverser: ReverserSpec,
    pub start: StartSpec,
    pub oil: OilSpec,
    pub limits: Limits,
}

/// Full-authority digital engine control: N1 governing through fuel flow,
/// with acceleration / deceleration limits expressed as fuel-to-burner-
/// pressure ratio (Wf/Ps3) schedules, the way real FADECs do it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FadecSpec {
    /// Ground idle corrected N1 (%)
    pub idle_n1_ground_pct: f64,
    /// Flight idle corrected N1 at sea level (%), used once airborne (Mach > 0.15)
    pub idle_n1_flight_pct: f64,
    /// Idle N1 increase per km of altitude (%/km)
    pub idle_n1_per_km: f64,
    /// Idle N1 increment with engine anti-ice on (%)
    pub idle_n1_anti_ice_pct: f64,
    /// Thrust-lever to N1 shaping exponent (N1 = idle + (max-idle) * tla^k)
    pub tla_exponent: f64,
    /// Proportional governor gain: kg/s of fuel per % N1 error
    pub governor_gain_kg_s_per_pct: f64,
    /// Acceleration limit: max Wf/Ps3 [(kg/h)/kPa] vs corrected N2 (%)
    pub accel_wf_p3_vs_n2c: Vec<[f64; 2]>,
    /// Deceleration limit: min Wf/Ps3 [(kg/h)/kPa] vs corrected N2 (%) (lean blow-out protection)
    pub decel_wf_p3_vs_n2c: Vec<[f64; 2]>,
    /// Absolute minimum fuel flow while running (kg/s)
    pub min_fuel_kg_s: f64,
    /// Fuel metering unit response lag (s)
    pub fuel_lag_s: f64,
    /// Flat rating: above the corner-point OAT the FADEC holds takeoff EGT
    /// (rated EGT at ISA + flat rating delta) by reducing N1.
    pub egt_flat_rating: bool,
}

/// Bleed air and variable geometry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BleedSpec {
    /// Engine (cowl) anti-ice bleed, fraction of core flow at station 3
    pub anti_ice_fraction: f64,
    /// Air conditioning pack bleed, fraction of core flow at station 3
    pub pack_fraction: f64,
    /// Variable bleed valves: fraction open vs corrected N2 (%); dumps booster
    /// exit flow into the bypass duct at low speed to keep the booster away
    /// from its surge line.
    pub vbv_open_vs_n2c: Vec<[f64; 2]>,
    /// Booster flow dumped when the VBVs are fully open, fraction of core flow
    pub vbv_dump_fraction: f64,
    /// Variable stator vane angle (deg from fully open) vs corrected N2 (%)
    pub vsv_angle_vs_n2c: Vec<[f64; 2]>,
    /// Surge margin at the design point (%), fan and HPC
    pub fan_surge_margin_pct: f64,
    pub hpc_surge_margin_pct: f64,
}

/// Thrust reverser (fan-air blocker doors / cascade on the CFM56-7B).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverserSpec {
    pub deploy_time_s: f64,
    /// Fraction of bypass thrust turned into reverse thrust at full deployment
    pub efficiency: f64,
    /// Maximum N1 in reverse (%)
    pub max_n1_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Turbofan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rating {
    /// Sea-level static takeoff thrust, ISA (N)
    pub takeoff_thrust_n: f64,
    /// Max continuous thrust, sea-level static (N)
    pub max_continuous_thrust_n: f64,
    /// Takeoff rating is flat-rated up to ISA + this many K
    pub flat_rating_delta_isa_k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geometry {
    pub fan_diameter_m: f64,
    pub length_m: f64,
    pub dry_mass_kg: f64,
    pub fan_stages: u32,
    pub booster_stages: u32,
    pub hpc_stages: u32,
    pub hpt_stages: u32,
    pub lpt_stages: u32,
}

/// Thermodynamic design point: sea level, static, ISA, takeoff rating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignPoint {
    /// Fan speed corresponding to 100% N1 (rpm)
    pub n1_100pct_rpm: f64,
    /// Core speed corresponding to 100% N2 (rpm)
    pub n2_100pct_rpm: f64,
    /// Total inlet mass flow at takeoff (kg/s)
    pub mass_flow_kg_s: f64,
    pub bypass_ratio: f64,
    /// Fan pressure ratio, bypass stream
    pub fan_pressure_ratio: f64,
    /// Fan hub + booster pressure ratio (core stream, station 2 -> 25)
    pub booster_pressure_ratio: f64,
    pub hpc_pressure_ratio: f64,
    /// Polytropic-ish isentropic efficiencies at the design point
    pub eta_fan: f64,
    pub eta_booster: f64,
    pub eta_hpc: f64,
    pub eta_burner: f64,
    pub eta_hpt: f64,
    pub eta_lpt: f64,
    pub eta_mechanical: f64,
    /// Combustor total pressure loss fraction
    pub burner_pressure_loss: f64,
    /// Fraction of HPC exit flow used for turbine cooling (bypasses the burner,
    /// re-enters downstream of the HPT)
    pub cooling_bleed_fraction: f64,
    /// Shaft power extracted from the HP spool for accessories (W)
    pub power_extraction_w: f64,
    /// Fuel lower heating value (J/kg), Jet A-1
    pub fuel_lhv_j_kg: f64,
    /// Off-design scaling exponents (component characteristics vs corrected speed)
    pub fan_flow_exponent: f64,
    pub fan_pr_exponent: f64,
    pub core_flow_exponent: f64,
    pub hpc_pr_exponent: f64,
    /// Component efficiency fall-off away from design speed:
    /// eta = eta_design * (1 - k * (1 - n)^2), with n the normalised corrected speed.
    pub compressor_efficiency_falloff: f64,
    pub turbine_efficiency_falloff: f64,
    /// EGT probe location expressed as the fraction of LPT temperature drop
    /// that has already occurred where the thermocouples sit (CFM56-7B: LPT
    /// stage 2 nozzle, early in the LPT).
    pub egt_probe_lpt_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spools {
    /// Ground idle (corrected) speeds (%)
    pub n1_idle_pct: f64,
    pub n2_idle_pct: f64,
    /// Steady-state N2 as a function of N1 (both % corrected). Linear interpolation.
    pub n2_vs_n1: Vec<[f64; 2]>,
    /// HP spool lag toward its steady-state speed line (s)
    pub n2_lag_s: f64,
    /// Polar moments of inertia (kg m^2). The LP spool integrates the LPT /
    /// fan power imbalance; the HP spool follows its operating line with a lag.
    pub lp_inertia_kg_m2: f64,
    pub hp_inertia_kg_m2: f64,
    /// Spool-down time constants after fuel cutoff (s)
    pub n1_spooldown_tau_s: f64,
    pub n2_spooldown_tau_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSpec {
    /// Max N2 the pneumatic starter can motor the core to (%)
    pub starter_max_n2_pct: f64,
    /// Starter time constant (s): N2 approaches the max motoring speed exponentially
    pub starter_tau_s: f64,
    /// Minimum N2 for fuel/ignition (%)
    pub min_light_n2_pct: f64,
    /// Delay from fuel-on to light-off (s)
    pub light_off_delay_s: f64,
    /// Highest altitude at which a light-off is possible (m)
    pub max_light_altitude_m: f64,
    /// Windmilling core speed: N2 % = windmill_n2_gain * Mach^windmill_n2_exp
    pub windmill_n2_gain: f64,
    pub windmill_n2_exp: f64,
    /// Windmilling fan speed: N1 % per unit Mach
    pub windmill_n1_per_mach: f64,
    /// Starter cutout N2 (%)
    pub starter_cutout_n2_pct: f64,
    /// Sub-idle fuel schedule: [N2 %, fuel flow kg/s]. The schedule is scaled
    /// at run time so that its value at idle N2 equals the steady idle fuel
    /// flow given by the cycle (continuity into governed running).
    pub start_fuel_schedule: Vec<[f64; 2]>,
    /// Gross sub-idle N2 acceleration from the turbine (%/s) vs N2 (%). Rotor
    /// drag (N2 / 12 %/s) and starter assist are subtracted / added at run time,
    /// so a hung start is the point where this curve meets the drag line.
    pub start_accel_vs_n2: Vec<[f64; 2]>,
    /// Fan/core speed ratio when the core is motored without combustion
    pub motoring_n1_over_n2: f64,
    /// Fan/core speed ratio sub-idle once lit
    pub lit_n1_over_n2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OilSpec {
    /// Tank capacity and normal full quantity (US quarts)
    pub capacity_qt: f64,
    pub full_qt: f64,
    /// Oil pressure model: psi = gain * N2% + offset, hot-oil derate per K above 90C
    pub pressure_gain_psi_per_pct: f64,
    pub pressure_offset_psi: f64,
    pub pressure_temp_derate_psi_per_k: f64,
    /// Oil temperature steady-state: ambient + idle rise + (to rise - idle rise) * load
    pub temp_rise_idle_k: f64,
    pub temp_rise_takeoff_k: f64,
    pub temp_tau_s: f64,
    /// Quantity "gulp" when running (qt held in the engine, not in the tank)
    pub gulp_qt: f64,
    /// Below this quantity the pressure starts to fall (qt)
    pub starvation_qt: f64,
    /// Seconds of operation without oil pressure before bearing failure
    pub seizure_time_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    pub n1_max_pct: f64,
    pub n2_max_pct: f64,
    /// EGT limits (C)
    pub egt_takeoff_c: f64,
    pub egt_max_continuous_c: f64,
    pub egt_start_c: f64,
    /// Oil
    pub oil_pressure_min_psi: f64,
    pub oil_pressure_caution_psi: f64,
    pub oil_temp_max_continuous_c: f64,
    pub oil_temp_max_transient_c: f64,
    /// Vibration advisory (units, cockpit scale 0-5)
    pub vib_advisory: f64,
    /// Takeoff rating time limit (s)
    pub takeoff_time_limit_s: f64,
}

impl EngineSpec {
    /// CFM56-7B26: 26,300 lbf takeoff thrust, Boeing 737-800/900.
    pub fn cfm56_7b26() -> Self {
        EngineSpec {
            name: "CFM56-7B26".into(),
            manufacturer: "CFM International".into(),
            application: "Boeing 737-800 / 737-900".into(),
            kind: EngineKind::Turbofan,
            rating: Rating {
                takeoff_thrust_n: 116_988.0,        // 26,300 lbf
                max_continuous_thrust_n: 105_867.0, // 23,800 lbf
                flat_rating_delta_isa_k: 15.0,       // flat rated to ISA+15 (30 C at SL)
            },
            geometry: Geometry {
                fan_diameter_m: 1.549, // 61.0 in
                length_m: 2.507,       // 98.7 in
                dry_mass_kg: 2_366.0,  // 5,216 lb
                fan_stages: 1,
                booster_stages: 3,
                hpc_stages: 9,
                hpt_stages: 1,
                lpt_stages: 4,
            },
            design_point: DesignPoint {
                n1_100pct_rpm: 5_175.0,
                n2_100pct_rpm: 14_460.0,
                mass_flow_kg_s: 355.0,
                bypass_ratio: 5.1,
                fan_pressure_ratio: 1.65,
                booster_pressure_ratio: 2.95,
                hpc_pressure_ratio: 11.1, // OPR = 2.95 * 11.1 = 32.7
                eta_fan: 0.90,
                eta_booster: 0.87,
                eta_hpc: 0.85,
                eta_burner: 0.995,
                eta_hpt: 0.88,
                eta_lpt: 0.90,
                eta_mechanical: 0.99,
                burner_pressure_loss: 0.05,
                cooling_bleed_fraction: 0.12,
                power_extraction_w: 150_000.0,
                fuel_lhv_j_kg: 43.0e6,
                fan_flow_exponent: 0.90,
                fan_pr_exponent: 1.90,
                core_flow_exponent: 2.80,
                hpc_pr_exponent: 2.00,
                compressor_efficiency_falloff: 0.70,
                turbine_efficiency_falloff: 0.30,
                egt_probe_lpt_fraction: 0.15,
            },
            spools: Spools {
                n1_idle_pct: 21.0,
                n2_idle_pct: 60.0,
                n2_vs_n1: vec![
                    [0.0, 0.0],
                    [21.0, 60.0],
                    [35.0, 70.0],
                    [50.0, 78.0],
                    [70.0, 86.5],
                    [85.0, 92.0],
                    [100.0, 97.5],
                    [104.0, 99.5],
                    [110.0, 102.0],
                ],
                n2_lag_s: 0.6,
                lp_inertia_kg_m2: 40.0,
                hp_inertia_kg_m2: 3.2,
                n1_spooldown_tau_s: 45.0,
                n2_spooldown_tau_s: 18.0,
            },
            fadec: FadecSpec {
                idle_n1_ground_pct: 21.0,
                idle_n1_flight_pct: 27.0,
                idle_n1_per_km: 1.2,
                idle_n1_anti_ice_pct: 4.0,
                tla_exponent: 1.3,
                governor_gain_kg_s_per_pct: 0.012,
                accel_wf_p3_vs_n2c: vec![
                    [50.0, 1.80],
                    [60.0, 1.95],
                    [70.0, 2.20],
                    [80.0, 2.40],
                    [90.0, 2.50],
                    [100.0, 2.50],
                    [105.0, 2.50],
                ],
                decel_wf_p3_vs_n2c: vec![
                    [50.0, 0.92],
                    [60.0, 0.96],
                    [80.0, 1.02],
                    [100.0, 1.05],
                    [105.0, 1.05],
                ],
                min_fuel_kg_s: 0.08,
                fuel_lag_s: 0.15,
                egt_flat_rating: true,
            },
            bleed: BleedSpec {
                anti_ice_fraction: 0.025,
                pack_fraction: 0.045,
                vbv_open_vs_n2c: vec![[0.0, 1.0], [62.0, 1.0], [75.0, 0.5], [86.0, 0.0], [110.0, 0.0]],
                vbv_dump_fraction: 0.20,
                vsv_angle_vs_n2c: vec![[0.0, 40.0], [60.0, 40.0], [75.0, 28.0], [88.0, 10.0], [96.0, 0.0], [110.0, 0.0]],
                fan_surge_margin_pct: 20.0,
                hpc_surge_margin_pct: 22.0,
            },
            reverser: ReverserSpec {
                deploy_time_s: 2.0,
                efficiency: 0.55,
                max_n1_pct: 85.0,
            },
            start: StartSpec {
                starter_max_n2_pct: 28.0,
                starter_tau_s: 7.0,
                min_light_n2_pct: 20.0,
                light_off_delay_s: 2.0,
                max_light_altitude_m: 9_144.0, // 30,000 ft
                windmill_n2_gain: 60.0,
                windmill_n2_exp: 1.2,
                windmill_n1_per_mach: 40.0,
                starter_cutout_n2_pct: 56.0,
                start_fuel_schedule: vec![
                    [20.0, 0.020],
                    [30.0, 0.030],
                    [45.0, 0.065],
                    [55.0, 0.110],
                    [60.0, 0.150],
                    [62.0, 0.155],
                ],
                start_accel_vs_n2: vec![
                    [20.0, 1.2],
                    [26.0, 2.3],
                    [30.0, 2.5],
                    [40.0, 2.9],
                    [50.0, 5.0],
                    [56.0, 6.5],
                    [60.0, 6.2],
                    [62.0, 5.8],
                ],
                motoring_n1_over_n2: 0.06,
                lit_n1_over_n2: 0.35,
            },
            oil: OilSpec {
                capacity_qt: 22.0,
                full_qt: 20.0,
                pressure_gain_psi_per_pct: 0.78,
                pressure_offset_psi: -2.0,
                pressure_temp_derate_psi_per_k: 0.12,
                temp_rise_idle_k: 70.0,
                temp_rise_takeoff_k: 100.0,
                temp_tau_s: 150.0,
                gulp_qt: 1.5,
                starvation_qt: 3.0,
                seizure_time_s: 90.0,
            },
            limits: Limits {
                n1_max_pct: 104.0,
                n2_max_pct: 105.0,
                egt_takeoff_c: 950.0,
                egt_max_continuous_c: 925.0,
                egt_start_c: 725.0,
                oil_pressure_min_psi: 13.0,
                oil_pressure_caution_psi: 20.0,
                oil_temp_max_continuous_c: 155.0,
                oil_temp_max_transient_c: 160.0,
                vib_advisory: 4.0,
                takeoff_time_limit_s: 300.0,
            },
        }
    }

    pub fn overall_pressure_ratio(&self) -> f64 {
        self.design_point.booster_pressure_ratio * self.design_point.hpc_pressure_ratio
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("spec serialises")
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }
}

/// Piecewise-linear interpolation on a sorted table of [x, y] rows, clamped at the ends.
pub fn interp(table: &[[f64; 2]], x: f64) -> f64 {
    if table.is_empty() {
        return 0.0;
    }
    if x <= table[0][0] {
        return table[0][1];
    }
    for w in table.windows(2) {
        let (x0, y0, x1, y1) = (w[0][0], w[0][1], w[1][0], w[1][1]);
        if x <= x1 {
            if (x1 - x0).abs() < 1e-12 {
                return y1;
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    table[table.len() - 1][1]
}
