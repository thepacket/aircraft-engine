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
    pub start: StartSpec,
    pub oil: OilSpec,
    pub limits: Limits,
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
    /// Max N1 acceleration (%/s) as a function of N1 (%). This is what the
    /// FADEC acceleration schedule (Wf/P3 limit) produces at the shaft.
    pub accel_rate_vs_n1: Vec<[f64; 2]>,
    /// Max N1 deceleration (%/s, positive numbers) vs N1 (%).
    pub decel_rate_vs_n1: Vec<[f64; 2]>,
    /// Response lags (s)
    pub n1_lag_s: f64,
    pub n2_lag_s: f64,
    /// Polar moments of inertia (kg m^2), used for the transient fuel term
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
    /// Starter cutout N2 (%)
    pub starter_cutout_n2_pct: f64,
    /// Sub-idle fuel schedule: [N2 %, fuel flow kg/s]. The schedule is scaled
    /// at run time so that its value at idle N2 equals the steady idle fuel
    /// flow given by the cycle (continuity into governed running).
    pub start_fuel_schedule: Vec<[f64; 2]>,
    /// Sub-idle N2 acceleration contributed by combustion (%/s) vs N2 (%)
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
                accel_rate_vs_n1: vec![
                    [0.0, 3.0],
                    [21.0, 4.0],
                    [30.0, 7.0],
                    [40.0, 12.0],
                    [60.0, 18.0],
                    [80.0, 18.0],
                    [95.0, 12.0],
                    [104.0, 8.0],
                    [110.0, 8.0],
                ],
                decel_rate_vs_n1: vec![
                    [0.0, 4.0],
                    [21.0, 4.0],
                    [30.0, 8.0],
                    [50.0, 15.0],
                    [80.0, 15.0],
                    [104.0, 10.0],
                    [110.0, 10.0],
                ],
                n1_lag_s: 0.6,
                n2_lag_s: 0.45,
                lp_inertia_kg_m2: 32.0,
                hp_inertia_kg_m2: 3.2,
                n1_spooldown_tau_s: 45.0,
                n2_spooldown_tau_s: 18.0,
            },
            start: StartSpec {
                starter_max_n2_pct: 28.0,
                starter_tau_s: 7.0,
                min_light_n2_pct: 20.0,
                light_off_delay_s: 2.0,
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
                    [20.0, 0.4],
                    [30.0, 0.9],
                    [40.0, 1.6],
                    [50.0, 2.2],
                    [58.0, 1.6],
                    [62.0, 0.8],
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
