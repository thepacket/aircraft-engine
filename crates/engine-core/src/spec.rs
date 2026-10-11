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
    /// Transient EGT limiting during accelerations: within this many degrees
    /// of the takeoff EGT limit the EEC blends the acceleration fuel limit
    /// down to the steady fuel. 0 disables it (schedule-only limiting).
    #[serde(default)]
    pub egt_transient_margin_c: f64,
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
    /// Fan drive gear ratio (LP turbine rpm / fan rpm); 1 for a direct-drive
    /// fan. N1 is the fan speed; the LP spool inertia is referred to the fan.
    #[serde(default = "one")]
    pub fan_gear_ratio: f64,
}

fn one() -> f64 {
    1.0
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
                fan_gear_ratio: 1.0,
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
                egt_transient_margin_c: 0.0,
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

    /// CFM LEAP-1B28: 29,317 lbf takeoff thrust, Boeing 737 MAX 8/9.
    ///
    /// Published figures (FAA TCDS E00088EN, Boeing 737 MAX FCOM limitations,
    /// CFM published data): rating, geometry, 100 % speeds, bypass ratio,
    /// overall pressure ratio class, EGT / speed / oil limits. The cycle and
    /// dynamic calibration knobs start from the CFM56-7B26 values and are
    /// adjusted for a higher bypass, higher pressure ratio, two-stage HPT
    /// engine of the 2010s; they are approximations, not CFM data.
    pub fn leap_1b28() -> Self {
        let mut s = Self::cfm56_7b26();
        s.name = "LEAP-1B28".into();
        s.manufacturer = "CFM International".into();
        s.application = "Boeing 737 MAX 8 / MAX 9".into();
        s.rating = Rating {
            takeoff_thrust_n: 130_410.0,        // 29,317 lbf
            max_continuous_thrust_n: 120_100.0, // ~27,000 lbf
            flat_rating_delta_isa_k: 15.0,
        };
        s.geometry = Geometry {
            fan_diameter_m: 1.763, // 69.4 in
            length_m: 3.147,       // 123.9 in
            dry_mass_kg: 2_780.0,  // 6,129 lb
            fan_stages: 1,
            booster_stages: 3,
            hpc_stages: 10,
            hpt_stages: 2,
            lpt_stages: 5,
        };
        let d = &mut s.design_point;
        d.n1_100pct_rpm = 3_894.0;
        d.n2_100pct_rpm = 19_391.0;
        d.mass_flow_kg_s = 460.0;
        d.bypass_ratio = 8.6;
        d.fan_pressure_ratio = 1.52;
        d.booster_pressure_ratio = 2.90;
        d.hpc_pressure_ratio = 14.0; // OPR = 2.90 * 14.0 = 40.6
        d.eta_fan = 0.92;
        d.eta_booster = 0.88;
        d.eta_hpc = 0.87;
        d.eta_hpt = 0.89;
        d.eta_lpt = 0.91;
        d.cooling_bleed_fraction = 0.16; // two cooled HPT stages, higher T4
        d.power_extraction_w = 180_000.0;
        s.spools.n1_idle_pct = 19.0;
        s.spools.n2_idle_pct = 58.0;
        // 100 % N2 is defined low on the LEAP-1B (redline 117.5 %): the core
        // runs above 100 % at takeoff.
        s.spools.n2_vs_n1 = vec![
            [0.0, 0.0],
            [19.0, 58.0],
            [35.0, 70.0],
            [50.0, 80.0],
            [70.0, 92.0],
            [85.0, 101.0],
            [100.0, 110.0],
            [104.0, 112.5],
            [110.0, 116.0],
        ];
        s.spools.lp_inertia_kg_m2 = 70.0; // larger, slower fan
        s.spools.hp_inertia_kg_m2 = 2.6;  // smaller, faster core
        s.spools.n1_spooldown_tau_s = 55.0;
        s.spools.n2_spooldown_tau_s = 16.0;
        s.fadec.idle_n1_ground_pct = 19.0;
        s.fadec.idle_n1_flight_pct = 25.0;
        // Wf/Ps3 schedules sit lower than the CFM56's: at OPR 40 the same fuel
        // flow is a smaller fraction of the burner pressure.
        s.fadec.accel_wf_p3_vs_n2c = vec![[50.0, 1.25], [60.0, 1.35], [70.0, 1.50], [80.0, 1.60], [90.0, 1.65], [100.0, 1.65], [120.0, 1.65]];
        s.fadec.decel_wf_p3_vs_n2c = vec![[50.0, 0.70], [60.0, 0.74], [80.0, 0.78], [100.0, 0.80], [120.0, 0.80]];
        s.bleed.vbv_open_vs_n2c = vec![[0.0, 1.0], [60.0, 1.0], [75.0, 0.5], [90.0, 0.0], [120.0, 0.0]];
        s.bleed.vsv_angle_vs_n2c = vec![[0.0, 40.0], [58.0, 40.0], [75.0, 28.0], [92.0, 10.0], [104.0, 0.0], [120.0, 0.0]];
        s.start.starter_max_n2_pct = 30.0;
        s.start.min_light_n2_pct = 22.0;
        s.start.starter_cutout_n2_pct = 60.0;
        s.start.start_fuel_schedule = vec![[22.0, 0.022], [30.0, 0.032], [45.0, 0.068], [55.0, 0.115], [58.0, 0.150], [60.0, 0.158]];
        s.start.start_accel_vs_n2 = vec![[22.0, 1.2], [26.0, 2.2], [30.0, 2.4], [40.0, 2.7], [50.0, 4.2], [58.0, 5.6], [60.0, 5.4], [62.0, 5.0]];
        s.oil.capacity_qt = 21.0;
        s.oil.full_qt = 19.0;
        s.limits = Limits {
            n1_max_pct: 104.0,
            n2_max_pct: 117.5,
            egt_takeoff_c: 1_038.0,
            egt_max_continuous_c: 1_013.0,
            egt_start_c: 750.0,
            oil_pressure_min_psi: 17.0,
            oil_pressure_caution_psi: 25.0,
            oil_temp_max_continuous_c: 140.0,
            oil_temp_max_transient_c: 155.0,
            vib_advisory: 4.0,
            takeoff_time_limit_s: 300.0,
        };
        s
    }

    /// IAE V2527-A5: 26,500 lbf takeoff thrust, Airbus A320-200.
    ///
    /// Published figures (FAA TCDS E40NE, Airbus A320 FCOM limitations, IAE
    /// data): rating, geometry, 100 % speeds, bypass ratio, pressure-ratio
    /// class, EGT / speed / oil limits. The V2500 reads EGT at the LPT exit,
    /// hence the low EGT numbers and the probe fraction near 1. Calibration
    /// knobs start from the CFM56-7B26 and are approximations.
    pub fn v2527_a5() -> Self {
        let mut s = Self::cfm56_7b26();
        s.name = "V2527-A5".into();
        s.manufacturer = "IAE International Aero Engines".into();
        s.application = "Airbus A320-200".into();
        s.rating = Rating {
            takeoff_thrust_n: 117_900.0,        // 26,500 lbf
            max_continuous_thrust_n: 108_000.0, // ~24,300 lbf
            flat_rating_delta_isa_k: 15.0,
        };
        s.geometry = Geometry {
            fan_diameter_m: 1.613, // 63.5 in
            length_m: 3.200,
            dry_mass_kg: 2_370.0,
            fan_stages: 1,
            booster_stages: 4,
            hpc_stages: 10,
            hpt_stages: 2,
            lpt_stages: 5,
        };
        let d = &mut s.design_point;
        d.n1_100pct_rpm = 5_650.0;
        d.n2_100pct_rpm = 14_950.0;
        d.mass_flow_kg_s = 355.0;
        d.bypass_ratio = 4.8;
        d.fan_pressure_ratio = 1.70;
        d.booster_pressure_ratio = 2.55;
        d.hpc_pressure_ratio = 10.8; // OPR = 2.55 * 10.8 = 27.5
        d.eta_fan = 0.90;
        d.eta_hpc = 0.86;
        d.eta_hpt = 0.89;
        d.cooling_bleed_fraction = 0.14;
        d.egt_probe_lpt_fraction = 0.95; // thermocouples at the LPT exit
        s.spools.n1_idle_pct = 20.0;
        s.spools.n2_idle_pct = 58.0;
        s.spools.n2_vs_n1 = vec![[0.0, 0.0], [20.0, 58.0], [35.0, 69.0], [50.0, 77.0], [70.0, 86.0], [85.0, 92.0], [100.0, 97.0], [104.0, 99.0], [110.0, 101.5]];
        s.fadec.idle_n1_ground_pct = 20.0;
        s.fadec.idle_n1_flight_pct = 26.0;
        // Lower OPR: the same fuel flow is a larger fraction of burner pressure
        s.fadec.accel_wf_p3_vs_n2c = vec![[50.0, 2.25], [60.0, 2.40], [70.0, 2.70], [80.0, 2.95], [90.0, 3.10], [100.0, 3.10], [105.0, 3.10]];
        s.fadec.egt_transient_margin_c = 35.0; // EEC trims the accel fuel as EGT nears the limit
        s.fadec.decel_wf_p3_vs_n2c = vec![[50.0, 0.98], [60.0, 1.03], [80.0, 1.12], [100.0, 1.18], [105.0, 1.18]]; // idle steady ~1.21
        s.start.min_light_n2_pct = 22.0;
        s.start.starter_cutout_n2_pct = 50.0;
        s.start.start_fuel_schedule = vec![[20.0, 0.020], [30.0, 0.030], [45.0, 0.065], [55.0, 0.110], [58.0, 0.150], [60.0, 0.155]];
        s.oil.capacity_qt = 30.0;
        s.oil.full_qt = 26.0;
        s.oil.pressure_gain_psi_per_pct = 1.55;
        s.oil.pressure_offset_psi = -10.0;
        s.limits = Limits {
            n1_max_pct: 100.0,
            n2_max_pct: 100.0,
            egt_takeoff_c: 635.0,
            egt_max_continuous_c: 610.0,
            egt_start_c: 635.0,
            oil_pressure_min_psi: 60.0,
            oil_pressure_caution_psi: 65.0,
            oil_temp_max_continuous_c: 155.0,
            oil_temp_max_transient_c: 165.0,
            vib_advisory: 4.0,
            takeoff_time_limit_s: 300.0,
        };
        s
    }

    /// Pratt & Whitney PW1127G-JM: 27,075 lbf takeoff thrust, Airbus A320neo.
    ///
    /// Geared turbofan: the LP turbine drives the fan through a 3.0625:1
    /// reduction gearbox, so N1 (fan) runs around 3,000 rpm while the LPT
    /// spins three times faster. Published figures (FAA TCDS E00087EN,
    /// A320neo FCOM limitations, P&W data): rating, fan diameter, bypass
    /// ratio, gear ratio, limits. Calibration knobs start from the LEAP-1B28.
    pub fn pw1127g_jm() -> Self {
        let mut s = Self::leap_1b28();
        s.name = "PW1127G-JM".into();
        s.manufacturer = "Pratt & Whitney".into();
        s.application = "Airbus A320neo".into();
        s.rating = Rating {
            takeoff_thrust_n: 120_430.0,        // 27,075 lbf
            max_continuous_thrust_n: 112_000.0, // ~25,200 lbf
            flat_rating_delta_isa_k: 15.0,
        };
        s.geometry = Geometry {
            fan_diameter_m: 2.057, // 81 in
            length_m: 3.400,
            dry_mass_kg: 2_857.0,  // 6,300 lb
            fan_stages: 1,
            booster_stages: 3,
            hpc_stages: 8,
            hpt_stages: 2,
            lpt_stages: 3,
        };
        let d = &mut s.design_point;
        d.n1_100pct_rpm = 3_281.0; // fan
        d.n2_100pct_rpm = 22_300.0;
        d.fan_gear_ratio = 3.0625;
        d.mass_flow_kg_s = 540.0;
        d.bypass_ratio = 12.5;
        d.fan_pressure_ratio = 1.33; // low fan pressure ratio at takeoff, static
        d.booster_pressure_ratio = 2.60;
        d.hpc_pressure_ratio = 15.4; // OPR = 40
        d.eta_fan = 0.93;
        d.eta_lpt = 0.92; // fast, few-stage LPT
        s.spools.n1_idle_pct = 17.0;
        s.spools.n2_idle_pct = 60.0;
        s.spools.n2_vs_n1 = vec![[0.0, 0.0], [17.0, 60.0], [35.0, 72.0], [50.0, 80.0], [70.0, 89.0], [85.0, 94.5], [100.0, 98.5], [104.0, 100.0], [110.0, 102.0]];
        s.spools.lp_inertia_kg_m2 = 120.0; // large fan plus the LPT seen through the gear (x9.4)
        s.spools.hp_inertia_kg_m2 = 2.2;
        s.fadec.idle_n1_ground_pct = 17.0;
        s.fadec.idle_n1_flight_pct = 23.0;
        // Steady Wf/Ps3 at takeoff is only ~0.70 (kg/h)/kPa at OPR 40 with this fuel flow
        s.fadec.accel_wf_p3_vs_n2c = vec![[50.0, 1.20], [60.0, 1.25], [70.0, 1.40], [80.0, 1.50], [90.0, 1.55], [100.0, 1.55], [120.0, 1.55]];
        s.fadec.decel_wf_p3_vs_n2c = vec![[50.0, 0.40], [60.0, 0.43], [80.0, 0.50], [100.0, 0.55], [120.0, 0.55]]; // idle steady ~0.54
        s.fadec.egt_transient_margin_c = 60.0;
        s.start.start_fuel_schedule = vec![[22.0, 0.022], [30.0, 0.032], [45.0, 0.068], [55.0, 0.115], [60.0, 0.150], [62.0, 0.158]];
        s.start.starter_cutout_n2_pct = 58.0;
        s.start.motoring_n1_over_n2 = 0.04;
        s.start.lit_n1_over_n2 = 0.30;
        s.limits = Limits {
            n1_max_pct: 100.0,
            n2_max_pct: 100.0,
            egt_takeoff_c: 1_083.0,
            egt_max_continuous_c: 1_038.0,
            egt_start_c: 870.0,
            oil_pressure_min_psi: 36.0,
            oil_pressure_caution_psi: 45.0,
            oil_temp_max_continuous_c: 163.0,
            oil_temp_max_transient_c: 177.0,
            vib_advisory: 4.0,
            takeoff_time_limit_s: 300.0,
        };
        s
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
