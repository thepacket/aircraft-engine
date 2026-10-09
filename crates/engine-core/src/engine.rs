//! Time-domain engine model: FADEC command scheduling, two-spool dynamics,
//! start / run / shutdown sequencing, sensors with lag and noise, limit
//! monitoring and data logging.

use crate::atmosphere::{inlet as inlet_conditions, isa, Inlet};
use crate::cycle::{self, cycle, CycleInput, CycleResult, DesignSizing};
use crate::logger::DataLogger;
use crate::spec::{interp, EngineSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Controls {
    /// Thrust lever angle, 0 = idle, 1 = full forward (takeoff / go-around)
    pub tla: f64,
    /// Engine start lever / fuel control switch: true = RUN, false = CUTOFF
    pub fuel_lever: bool,
    /// Starter engaged (ground pneumatic start)
    pub starter: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Controls { tla: 0.0, fuel_lever: false, starter: false }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Environment {
    pub altitude_m: f64,
    pub mach: f64,
    pub delta_isa_k: f64,
}

impl Default for Environment {
    fn default() -> Self {
        Environment { altitude_m: 0.0, mach: 0.0, delta_isa_k: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Spools stopped (or windmilling), no fuel
    Off,
    /// Starter cranking the core, no fuel
    Motoring,
    /// Fuel on, sub-idle acceleration to idle
    Starting,
    /// Idle or above, FADEC governing N1
    Running,
    /// Fuel cut, spools decaying
    Shutdown,
}

/// Everything an instrument, a logger or a student might want to look at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub time_s: f64,
    pub mode: Mode,
    pub controls: Controls,
    pub environment: Environment,
    pub inlet: Inlet,
    pub n1_pct: f64,
    pub n2_pct: f64,
    pub n1_rpm: f64,
    pub n2_rpm: f64,
    pub n1_corrected_pct: f64,
    pub n2_corrected_pct: f64,
    pub n1_command_pct: f64,
    pub n1_dot_pct_s: f64,
    pub n2_dot_pct_s: f64,
    /// Indicated EGT (thermocouple with lag), C
    pub egt_c: f64,
    /// True gas temperature at the EGT probe, C
    pub egt_gas_c: f64,
    pub t4_k: f64,
    /// Indicated fuel flow (kg/s and lb/h)
    pub fuel_flow_kg_s: f64,
    pub fuel_flow_pph: f64,
    /// Steady-state fuel flow from the cycle (without the transient term)
    pub fuel_flow_steady_kg_s: f64,
    pub fuel_used_kg: f64,
    pub thrust_n: f64,
    pub thrust_lbf: f64,
    /// Thrust specific fuel consumption, lb/(lbf h)
    pub tsfc_lb_lbf_h: f64,
    pub oil_pressure_psi: f64,
    pub oil_temperature_c: f64,
    pub oil_quantity_qt: f64,
    pub vib_n1: f64,
    pub vib_n2: f64,
    pub starter_engaged: bool,
    pub ignition: bool,
    pub lit: bool,
    pub takeoff_timer_s: f64,
    pub run_time_s: f64,
    pub cycle: CycleResult,
    pub warnings: Vec<String>,
    pub logger_rows: usize,
    pub logger_enabled: bool,
}

/// Small deterministic PRNG (xorshift64*) so runs are reproducible.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
    }
    /// Approximately normal, zero mean, unit variance
    fn gauss(&mut self) -> f64 {
        let s: f64 = (0..6).map(|_| self.next_f64()).sum();
        (s - 3.0) * (2.0f64).sqrt()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Engine {
    pub spec: EngineSpec,
    pub sizing: DesignSizing,
    pub controls: Controls,
    pub environment: Environment,
    pub logger: DataLogger,
    time_s: f64,
    mode: Mode,
    n1: f64,
    n2: f64,
    n1_dot: f64,
    n2_dot: f64,
    n1_cmd: f64,
    lit: bool,
    fuel_on_since: Option<f64>,
    starter_engaged: bool,
    t4: f64,
    egt_gas_k: f64,
    egt_sensed_k: f64,
    ff_sensed: f64,
    ff_steady: f64,
    fuel_used_kg: f64,
    oil_temp_k: f64,
    oil_press_psi: f64,
    takeoff_timer_s: f64,
    run_time_s: f64,
    rng: Rng,
    /// Steady idle fuel flow at sea level (kg/s), used to scale the start schedule
    idle_fuel_kg_s: f64,
    last_cycle: CycleResult,
    warnings: Vec<String>,
    max_substep_s: f64,
}

pub const LOG_COLUMNS: &[&str] = &[
    "time_s", "mode", "tla", "fuel_lever", "starter", "altitude_m", "mach", "delta_isa_k",
    "tt2_k", "pt2_pa", "n1_pct", "n2_pct", "n1_corrected_pct", "n2_corrected_pct", "n1_command_pct",
    "egt_c", "egt_gas_c", "t4_k", "fuel_flow_kg_s", "fuel_flow_pph", "fuel_used_kg",
    "thrust_n", "thrust_lbf", "tsfc_lb_lbf_h", "oil_pressure_psi", "oil_temperature_c", "oil_quantity_qt",
    "vib_n1", "vib_n2", "w2_kg_s", "w25_kg_s", "bypass_ratio", "fan_pr", "booster_pr", "hpc_pr", "opr",
    "tt13_k", "tt25_k", "tt3_k", "pt3_pa", "tt45_k", "tt5_k", "pt5_pa", "v9_m_s", "v19_m_s",
    "thermal_eff", "propulsive_eff", "overall_eff",
];

impl Engine {
    pub fn new(spec: EngineSpec) -> Self {
        let sizing = cycle::size_design_point(&spec);
        let env = Environment::default();
        let inl = inlet_conditions(isa(env.altitude_m, env.delta_isa_k), env.mach);
        let cold = cycle(&spec, &inl, CycleInput { n1c: 0.0, n2c: 0.0, t4_k: inl.tt2_k, a9_m2: Some(sizing.core_nozzle_area_m2), combustion: false });
        let columns = LOG_COLUMNS.iter().map(|s| s.to_string()).collect();
        let idle_fuel_kg_s = {
            let n1c = spec.spools.n1_idle_pct / 100.0;
            let n2c = interp(&spec.spools.n2_vs_n1, spec.spools.n1_idle_pct) / 100.0;
            let t4 = cycle::solve_t4_for_nozzle(&spec, &inl, n1c, n2c, sizing.core_nozzle_area_m2);
            cycle(&spec, &inl, CycleInput { n1c, n2c, t4_k: t4, a9_m2: Some(sizing.core_nozzle_area_m2), combustion: true }).wf_kg_s
        };
        Engine {
            spec,
            sizing,
            controls: Controls::default(),
            environment: env,
            logger: DataLogger::new(10.0, columns),
            time_s: 0.0,
            mode: Mode::Off,
            n1: 0.0,
            n2: 0.0,
            n1_dot: 0.0,
            n2_dot: 0.0,
            n1_cmd: 0.0,
            lit: false,
            fuel_on_since: None,
            starter_engaged: false,
            t4: inl.tt2_k,
            egt_gas_k: inl.tt2_k,
            egt_sensed_k: inl.tt2_k,
            ff_sensed: 0.0,
            ff_steady: 0.0,
            fuel_used_kg: 0.0,
            oil_temp_k: inl.tt2_k,
            oil_press_psi: 0.0,
            takeoff_timer_s: 0.0,
            run_time_s: 0.0,
            rng: Rng(0x9E3779B97F4A7C15),
            idle_fuel_kg_s,
            last_cycle: cold,
            warnings: Vec::new(),
            max_substep_s: 0.02,
        }
    }

    /// Reset to cold and dark, keeping the spec, controls and environment.
    pub fn reset(&mut self) {
        let spec = self.spec.clone();
        let controls = self.controls;
        let env = self.environment;
        let rate = self.logger.rate_hz;
        *self = Engine::new(spec);
        self.controls = controls;
        self.environment = env;
        self.logger.set_rate(rate);
    }

    /// Replace the engine specification (re-sizes the design point) and reset.
    pub fn set_spec(&mut self, spec: EngineSpec) {
        let controls = self.controls;
        let env = self.environment;
        let rate = self.logger.rate_hz;
        *self = Engine::new(spec);
        self.controls = controls;
        self.environment = env;
        self.logger.set_rate(rate);
    }

    /// Jump straight to a stabilised running condition (useful for tests and
    /// for starting a lesson at cruise without flying the start sequence).
    pub fn set_running(&mut self, tla: f64) {
        self.controls.fuel_lever = true;
        self.controls.starter = false;
        self.controls.tla = tla.clamp(0.0, 1.0);
        let inl = self.inlet();
        let n1c = self.n1_command_corrected(&inl);
        self.n1 = n1c * inl.theta.sqrt();
        self.n2 = interp(&self.spec.spools.n2_vs_n1, n1c) * inl.theta.sqrt();
        self.n1_cmd = self.n1;
        self.mode = Mode::Running;
        self.lit = true;
        self.n1_dot = 0.0;
        self.n2_dot = 0.0;
        let a9 = self.sizing.core_nozzle_area_m2;
        self.t4 = cycle::solve_t4_for_nozzle(&self.spec, &inl, n1c / 100.0, self.n2 / inl.theta.sqrt() / 100.0, a9);
        let r = cycle(&self.spec, &inl, CycleInput { n1c: n1c / 100.0, n2c: self.n2 / inl.theta.sqrt() / 100.0, t4_k: self.t4, a9_m2: Some(a9), combustion: true });
        self.egt_gas_k = r.egt_k;
        self.egt_sensed_k = r.egt_k;
        self.ff_sensed = r.wf_kg_s;
        self.ff_steady = r.wf_kg_s;
        self.oil_temp_k = self.oil_temp_target(&inl);
        self.last_cycle = r;
    }

    pub fn time(&self) -> f64 {
        self.time_s
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    fn inlet(&self) -> Inlet {
        inlet_conditions(isa(self.environment.altitude_m, self.environment.delta_isa_k), self.environment.mach)
    }

    /// FADEC N1 command schedule (corrected %, before the actual-speed limit).
    fn n1_command_corrected(&self, inl: &Inlet) -> f64 {
        let s = &self.spec.spools;
        let tla = self.controls.tla.clamp(0.0, 1.0);
        // Idle is held as a corrected speed; in flight idle rises with altitude
        // to keep bleed and the accel capability (approximation of flight idle).
        let idle_c = s.n1_idle_pct + 12.0 * (self.environment.altitude_m / 10_000.0).clamp(0.0, 1.0);
        // Takeoff rating: 100% corrected, flat rated. Above the flat-rating
        // temperature the FADEC holds EGT by limiting N1 (approximated here as
        // a linear N1 reduction of 0.5 %/K beyond ISA + flat rating).
        let excess = (inl.ambient.temperature_k - (isa(self.environment.altitude_m, 0.0).temperature_k + self.spec.rating.flat_rating_delta_isa_k)).max(0.0);
        let max_c = 100.0 - 0.5 * excess;
        let cmd_c = idle_c + (max_c - idle_c) * tla.powf(1.3);
        // Actual speed limit (overspeed protection)
        let max_actual_c = self.spec.limits.n1_max_pct / inl.theta.sqrt();
        cmd_c.min(max_actual_c)
    }

    fn oil_temp_target(&self, inl: &Inlet) -> f64 {
        let o = &self.spec.oil;
        let s = &self.spec.spools;
        if self.mode == Mode::Running || self.mode == Mode::Starting {
            let load = ((self.n2 - s.n2_idle_pct) / (100.0 - s.n2_idle_pct)).clamp(0.0, 1.0);
            inl.tt2_k + o.temp_rise_idle_k + (o.temp_rise_takeoff_k - o.temp_rise_idle_k) * load
        } else {
            inl.tt2_k
        }
    }

    /// Advance the simulation by `dt` seconds (internally sub-stepped).
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
        let inl = self.inlet();
        let sqrt_theta = inl.theta.sqrt();
        let spec = self.spec.clone();
        let s = &spec.spools;
        let st = &spec.start;
        let a9 = self.sizing.core_nozzle_area_m2;
        let c = self.controls;
        self.warnings.clear();

        // Windmilling floor in flight: the fan is driven by the airstream.
        let n1_windmill = (38.0 * self.environment.mach).clamp(0.0, 30.0);
        let n2_windmill = 0.35 * n1_windmill;

        // ---- Mode transitions ---------------------------------------------
        let fuel_ok = c.fuel_lever && self.n2 >= st.min_light_n2_pct;
        match self.mode {
            Mode::Off => {
                if c.starter {
                    self.mode = Mode::Motoring;
                } else if fuel_ok {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                }
            }
            Mode::Motoring => {
                if fuel_ok {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                } else if !c.starter {
                    self.mode = Mode::Off;
                }
            }
            Mode::Starting => {
                if !c.fuel_lever {
                    self.mode = Mode::Shutdown;
                    self.lit = false;
                    self.fuel_on_since = None;
                } else if self.lit && self.n2 >= s.n2_idle_pct * sqrt_theta - 0.3 {
                    self.mode = Mode::Running;
                    self.n1_cmd = self.n1;
                }
            }
            Mode::Running => {
                if !c.fuel_lever {
                    self.mode = Mode::Shutdown;
                    self.lit = false;
                    self.fuel_on_since = None;
                }
            }
            Mode::Shutdown => {
                if fuel_ok {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                } else if c.starter && self.n2 < st.starter_max_n2_pct {
                    self.mode = Mode::Motoring;
                } else if self.n2 < 1.0 {
                    // The fan may keep turning (or windmill) for minutes; the
                    // engine is "off" once the core has stopped.
                    self.mode = Mode::Off;
                }
            }
        }

        // Starter: engages when commanded below cutout speed, drops out above it.
        self.starter_engaged = c.starter && self.n2 < st.starter_cutout_n2_pct && matches!(self.mode, Mode::Motoring | Mode::Starting | Mode::Off | Mode::Shutdown);

        // ---- Spool dynamics -----------------------------------------------
        let n1_prev = self.n1;
        let n2_prev = self.n2;
        let mut wf_dyn;
        let combustion;

        match self.mode {
            Mode::Off | Mode::Motoring | Mode::Shutdown => {
                combustion = false;
                wf_dyn = 0.0;
                // HP spool: starter torque or free decay
                let n2_target_decay = n2_windmill;
                if self.starter_engaged {
                    self.n2 += (st.starter_max_n2_pct - self.n2) / st.starter_tau_s * h;
                } else {
                    self.n2 += (n2_target_decay - self.n2) / s.n2_spooldown_tau_s * h;
                }
                // LP spool: windmilling / residual gas drive
                let n1_target = if self.starter_engaged { st.motoring_n1_over_n2 * self.n2 } else { n1_windmill };
                let tau = if self.mode == Mode::Shutdown { s.n1_spooldown_tau_s } else { 8.0 };
                self.n1 += (n1_target.max(n1_windmill) - self.n1) / tau * h;
                self.t4 = inl.tt2_k;
            }
            Mode::Starting => {
                // Light-off after the ignition delay
                if let Some(t0) = self.fuel_on_since {
                    if !self.lit && self.time_s - t0 >= st.light_off_delay_s {
                        self.lit = true;
                    }
                }
                combustion = self.lit;
                // Start schedule, scaled so that it meets the governed idle fuel flow
                let sched_idle = interp(&st.start_fuel_schedule, s.n2_idle_pct).max(1e-6);
                let scale = self.idle_fuel_kg_s * inl.delta / sched_idle;
                wf_dyn = interp(&st.start_fuel_schedule, self.n2 / sqrt_theta) * scale;
                // Starter torque falls to zero at its max motoring speed; it
                // keeps assisting (never retarding) until cutout.
                let starter_assist = if self.starter_engaged { (st.starter_max_n2_pct - self.n2).max(0.0) / st.starter_tau_s } else { 0.0 };
                let mut n2_dot = starter_assist;
                if self.lit {
                    // Net acceleration from combustion (turbine torque minus compressor
                    // and friction drag), from the sub-idle schedule.
                    n2_dot += interp(&st.start_accel_vs_n2, self.n2 / sqrt_theta);
                } else {
                    n2_dot += (n2_windmill - self.n2).min(0.0) / s.n2_spooldown_tau_s;
                }
                self.n2 += n2_dot * h;
                let ratio = if self.lit { st.lit_n1_over_n2 } else { st.motoring_n1_over_n2 };
                let n1_target = (ratio * self.n2).max(n1_windmill);
                self.n1 += (n1_target - self.n1) / 2.5 * h;
            }
            Mode::Running => {
                combustion = true;
                // FADEC: N1 command, then rate limiting (accel / decel schedules)
                let n1c_cmd = self.n1_command_corrected(&inl);
                let cmd = n1c_cmd * sqrt_theta;
                let err = cmd - self.n1_cmd;
                let max_up = interp(&s.accel_rate_vs_n1, self.n1_cmd) * h;
                let max_dn = interp(&s.decel_rate_vs_n1, self.n1_cmd) * h;
                self.n1_cmd += err.clamp(-max_dn, max_up);
                // Spool responds to the rate-limited command with a first-order lag
                self.n1 += (self.n1_cmd - self.n1) / s.n1_lag_s * h;
                let n2_ss = interp(&s.n2_vs_n1, self.n1 / sqrt_theta) * sqrt_theta;
                self.n2 += (n2_ss - self.n2) / s.n2_lag_s * h;
                wf_dyn = 0.0; // filled below from the cycle
            }
        }
        self.n1 = self.n1.max(0.0);
        self.n2 = self.n2.max(0.0);
        self.n1_dot = (self.n1 - n1_prev) / h;
        self.n2_dot = (self.n2 - n2_prev) / h;

        // ---- Thermodynamic cycle ------------------------------------------
        let n1c = self.n1 / sqrt_theta / 100.0;
        let n2c = self.n2 / sqrt_theta / 100.0;
        let r = match self.mode {
            Mode::Running => {
                self.t4 = cycle::solve_t4_for_nozzle(&spec, &inl, n1c, n2c, a9);
                let r = cycle(&spec, &inl, CycleInput { n1c, n2c, t4_k: self.t4, a9_m2: Some(a9), combustion: true });
                // Transient fuel: the energy going into (or out of) spool kinetic energy
                let d = &spec.design_point;
                let w1 = self.n1 / 100.0 * d.n1_100pct_rpm * std::f64::consts::TAU / 60.0;
                let w2 = self.n2 / 100.0 * d.n2_100pct_rpm * std::f64::consts::TAU / 60.0;
                let a1 = self.n1_dot / 100.0 * d.n1_100pct_rpm * std::f64::consts::TAU / 60.0;
                let a2 = self.n2_dot / 100.0 * d.n2_100pct_rpm * std::f64::consts::TAU / 60.0;
                let p_accel = s.lp_inertia_kg_m2 * w1 * a1 + s.hp_inertia_kg_m2 * w2 * a2;
                let conv = r.thermal_efficiency.clamp(0.18, 0.45);
                let extra = p_accel / (conv * d.fuel_lhv_j_kg);
                self.ff_steady = r.wf_kg_s;
                wf_dyn = (r.wf_kg_s + extra).max(0.35 * r.wf_kg_s);
                r
            }
            Mode::Starting if self.lit => {
                let cs = cycle::cold_section(&spec, &inl, n1c, n2c);
                let w_b = cs.w25 * (1.0 - spec.design_point.cooling_bleed_fraction);
                self.t4 = cycle::t4_for_fuel(&spec, w_b, cs.tt3, wf_dyn);
                self.ff_steady = wf_dyn;
                cycle(&spec, &inl, CycleInput { n1c, n2c, t4_k: self.t4, a9_m2: Some(a9), combustion: true })
            }
            _ => {
                self.ff_steady = 0.0;
                cycle(&spec, &inl, CycleInput { n1c, n2c, t4_k: inl.tt2_k, a9_m2: Some(a9), combustion: false })
            }
        };

        // ---- Gas temperature at the EGT probe ------------------------------
        if combustion {
            let ratio = if self.ff_steady > 1e-6 { wf_dyn / self.ff_steady } else { 1.0 };
            let overshoot = 0.4 * (self.t4 - r.stations[4].tt_k) * (ratio - 1.0);
            let target = r.egt_k + overshoot;
            // Gas path responds fast; the combustor/turbine metal adds a little lag
            self.egt_gas_k += (target - self.egt_gas_k) / 0.35 * h;
        } else {
            // Residual heat soaks out with the airflow
            let tau = 25.0 + 60.0 / (1.0 + self.n2 / 10.0);
            self.egt_gas_k += (inl.tt2_k - self.egt_gas_k) / tau * h;
        }

        // ---- Sensors --------------------------------------------------------
        self.egt_sensed_k += (self.egt_gas_k - self.egt_sensed_k) / 1.5 * h;
        self.ff_sensed += (wf_dyn - self.ff_sensed) / 0.4 * h;
        self.fuel_used_kg += wf_dyn * h;

        let o = &spec.oil;
        let oil_target = self.oil_temp_target(&inl);
        self.oil_temp_k += (oil_target - self.oil_temp_k) / o.temp_tau_s * h;
        let oil_temp_c = self.oil_temp_k - 273.15;
        let press = o.pressure_gain_psi_per_pct * self.n2 + o.pressure_offset_psi - o.pressure_temp_derate_psi_per_k * (oil_temp_c - 90.0).max(0.0);
        self.oil_press_psi = press.max(0.0) + 0.15 * self.rng.gauss() * (self.n2 > 5.0) as u8 as f64;

        // ---- Timers ---------------------------------------------------------
        if self.mode == Mode::Running {
            self.run_time_s += h;
            if self.controls.tla > 0.9 {
                self.takeoff_timer_s += h;
            } else {
                self.takeoff_timer_s = (self.takeoff_timer_s - h).max(0.0);
            }
        }
        self.time_s += h;

        // ---- Limit monitoring -----------------------------------------------
        let l = &spec.limits;
        let egt_c = self.egt_sensed_k - 273.15;
        if self.mode == Mode::Starting && egt_c > l.egt_start_c {
            self.warnings.push(format!("EGT {:.0} C exceeds start limit {:.0} C", egt_c, l.egt_start_c));
        }
        if self.mode == Mode::Running {
            if egt_c > l.egt_takeoff_c {
                self.warnings.push(format!("EGT {:.0} C exceeds takeoff limit {:.0} C", egt_c, l.egt_takeoff_c));
            } else if egt_c > l.egt_max_continuous_c && self.takeoff_timer_s > l.takeoff_time_limit_s {
                self.warnings.push(format!("EGT above max continuous {:.0} C for more than {:.0} s", l.egt_max_continuous_c, l.takeoff_time_limit_s));
            }
            if self.n1 > l.n1_max_pct {
                self.warnings.push(format!("N1 {:.1}% exceeds limit {:.1}%", self.n1, l.n1_max_pct));
            }
            if self.n2 > l.n2_max_pct {
                self.warnings.push(format!("N2 {:.1}% exceeds limit {:.1}%", self.n2, l.n2_max_pct));
            }
            if self.oil_press_psi < l.oil_pressure_min_psi {
                self.warnings.push(format!("Oil pressure {:.0} psi below minimum {:.0} psi", self.oil_press_psi, l.oil_pressure_min_psi));
            }
            if oil_temp_c > l.oil_temp_max_continuous_c {
                self.warnings.push(format!("Oil temperature {:.0} C above {:.0} C", oil_temp_c, l.oil_temp_max_continuous_c));
            }
        }

        self.last_cycle = r;

        // ---- Logging ---------------------------------------------------------
        if self.logger.due(self.time_s) {
            let state = self.state_inner(&inl, wf_dyn);
            self.logger.maybe_sample(self.time_s, || log_row(&state));
        }
    }

    pub fn state(&self) -> EngineState {
        let inl = self.inlet();
        self.state_inner(&inl, self.ff_sensed)
    }

    fn state_inner(&self, inl: &Inlet, _wf_dyn: f64) -> EngineState {
        let d = &self.spec.design_point;
        let sqrt_theta = inl.theta.sqrt();
        let r = &self.last_cycle;
        let thrust = r.net_thrust_n;
        let tsfc = if thrust > 100.0 { self.ff_sensed / thrust * 3600.0 * 9.80665 } else { 0.0 };
        let o = &self.spec.oil;
        let n1f = self.n1 / 100.0;
        let n2f = self.n2 / 100.0;
        let mut rng = self.rng.clone();
        let vib_n1 = if self.n1 > 2.0 { (0.3 + 1.1 * n1f * n1f + 0.08 * rng.gauss()).max(0.0) } else { 0.0 };
        let vib_n2 = if self.n2 > 2.0 { (0.2 + 0.7 * n2f * n2f + 0.06 * rng.gauss()).max(0.0) } else { 0.0 };
        EngineState {
            time_s: self.time_s,
            mode: self.mode,
            controls: self.controls,
            environment: self.environment,
            inlet: *inl,
            n1_pct: self.n1,
            n2_pct: self.n2,
            n1_rpm: n1f * d.n1_100pct_rpm,
            n2_rpm: n2f * d.n2_100pct_rpm,
            n1_corrected_pct: self.n1 / sqrt_theta,
            n2_corrected_pct: self.n2 / sqrt_theta,
            n1_command_pct: self.n1_cmd,
            n1_dot_pct_s: self.n1_dot,
            n2_dot_pct_s: self.n2_dot,
            egt_c: self.egt_sensed_k - 273.15,
            egt_gas_c: self.egt_gas_k - 273.15,
            t4_k: self.t4,
            fuel_flow_kg_s: self.ff_sensed,
            fuel_flow_pph: self.ff_sensed * 7936.64,
            fuel_flow_steady_kg_s: self.ff_steady,
            fuel_used_kg: self.fuel_used_kg,
            thrust_n: thrust,
            thrust_lbf: thrust / 4.448222,
            tsfc_lb_lbf_h: tsfc,
            oil_pressure_psi: self.oil_press_psi,
            oil_temperature_c: self.oil_temp_k - 273.15,
            oil_quantity_qt: o.full_qt - o.gulp_qt * (self.n2 / self.spec.spools.n2_idle_pct).clamp(0.0, 1.0),
            vib_n1,
            vib_n2,
            starter_engaged: self.starter_engaged,
            ignition: self.mode == Mode::Starting,
            lit: self.lit,
            takeoff_timer_s: self.takeoff_timer_s,
            run_time_s: self.run_time_s,
            cycle: r.clone(),
            warnings: self.warnings.clone(),
            logger_rows: self.logger.len(),
            logger_enabled: self.logger.enabled,
        }
    }
}

fn mode_code(m: Mode) -> f64 {
    match m {
        Mode::Off => 0.0,
        Mode::Motoring => 1.0,
        Mode::Starting => 2.0,
        Mode::Running => 3.0,
        Mode::Shutdown => 4.0,
    }
}

/// One logger row, in the order of [`LOG_COLUMNS`].
pub fn log_row(s: &EngineState) -> Vec<f64> {
    let c = &s.cycle;
    let st = |id: &str| c.stations.iter().find(|x| x.id == id).cloned().unwrap_or_default();
    vec![
        s.time_s, mode_code(s.mode), s.controls.tla, s.controls.fuel_lever as u8 as f64, s.controls.starter as u8 as f64,
        s.environment.altitude_m, s.environment.mach, s.environment.delta_isa_k,
        s.inlet.tt2_k, s.inlet.pt2_pa, s.n1_pct, s.n2_pct, s.n1_corrected_pct, s.n2_corrected_pct, s.n1_command_pct,
        s.egt_c, s.egt_gas_c, s.t4_k, s.fuel_flow_kg_s, s.fuel_flow_pph, s.fuel_used_kg,
        s.thrust_n, s.thrust_lbf, s.tsfc_lb_lbf_h, s.oil_pressure_psi, s.oil_temperature_c, s.oil_quantity_qt,
        s.vib_n1, s.vib_n2, c.w2_kg_s, c.w25_kg_s, c.bypass_ratio, c.fan_pressure_ratio, c.booster_pressure_ratio, c.hpc_pressure_ratio, c.overall_pressure_ratio,
        st("13").tt_k, st("25").tt_k, st("3").tt_k, st("3").pt_pa, st("45").tt_k, st("5").tt_k, st("5").pt_pa,
        c.core_nozzle.exit_velocity_m_s, c.bypass_nozzle.exit_velocity_m_s,
        c.thermal_efficiency, c.propulsive_efficiency, c.overall_efficiency,
    ]
}
