//! Time-domain engine model: fuel-based FADEC (N1 governing through Wf with
//! Wf/Ps3 acceleration and deceleration schedules), two-spool dynamics,
//! start / run / shutdown sequencing with abnormal starts, flameout and
//! auto-relight, surge, bleed and variable geometry, thrust reverser, oil
//! system, fault injection, sensors with lag and noise, limit monitoring and
//! data logging.

use crate::atmosphere::{inlet as inlet_conditions, isa, Inlet};
use crate::cycle::{self, cycle, CycleInput, CycleResult, DesignSizing};
use crate::logger::DataLogger;
use crate::spec::{interp, EngineSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Controls {
    /// Thrust lever angle, 0 = idle, 1 = full forward (takeoff / go-around).
    /// With the reverser deployed it commands reverse thrust instead.
    pub tla: f64,
    /// Engine start lever / fuel control switch: true = RUN, false = CUTOFF
    pub fuel_lever: bool,
    /// Starter engaged (ground pneumatic start)
    pub starter: bool,
    /// Engine (cowl) anti-ice bleed on
    pub anti_ice: bool,
    /// Air-conditioning pack bleed on
    pub pack_bleed: bool,
    /// Thrust reverser lever: true = deploy
    pub reverser: bool,
    /// Direct N1 demand (%), as an autothrottle or a replayed flight log would
    /// command it. Negative = none (use the thrust lever).
    pub n1_demand_pct: f64,
}

impl Default for Controls {
    fn default() -> Self {
        Controls { tla: 0.0, fuel_lever: false, starter: false, anti_ice: false, pack_bleed: false, reverser: false, n1_demand_pct: -1.0 }
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

/// Injected faults for abnormal-procedure training. All default to "no fault".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Faults {
    /// Starter inoperative
    pub starter_inop: bool,
    /// Weak starter (low duct pressure): slow motoring, low max motoring speed
    pub starter_weak: bool,
    /// Starter drops out early (at 30% N2) -> hung start
    pub starter_early_cutout: bool,
    /// Igniters inoperative -> wet start (fuel, no light-off)
    pub ignition_fail: bool,
    /// Start fuel schedule multiplier: 1.0 normal, >1 rich (hot start), <1 lean (hung start)
    pub start_fuel_factor: f64,
    /// Fuel pump / fuel supply failure -> flameout
    pub fuel_pump_fail: bool,
    /// Oil leak (US quarts per minute)
    pub oil_leak_qt_per_min: f64,
    /// Engine fire
    pub fire: bool,
    /// N1 overspeed governor channel lost
    pub n1_governor_fail: bool,
    /// HPC surge margin lost to blade damage / deterioration (% points)
    pub compressor_damage_pct: f64,
    /// Foreign object damage: vibration, fan surge margin loss, EGT shift
    pub fod: bool,
    /// EGT thermocouple harness open -> no EGT indication
    pub egt_probe_fail: bool,
    /// Thrust reverser stuck at its current position
    pub reverser_stuck: bool,
    /// One-shot triggers (consumed on the next step)
    pub trigger_surge: bool,
    pub trigger_flameout: bool,
    /// Fire bottle discharge (one-shot)
    pub fire_bottle: bool,
}

impl Faults {
    pub fn none() -> Self {
        Faults { start_fuel_factor: 1.0, ..Default::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Spools stopped (or windmilling), no fuel
    Off,
    /// Starter cranking the core, no fuel
    Motoring,
    /// Fuel on, ignition on, sub-idle (ground start, windmill start or auto-relight)
    Starting,
    /// Idle or above, FADEC governing N1 through fuel flow
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
    pub faults: Faults,
    pub inlet: Inlet,
    pub n1_pct: f64,
    pub n2_pct: f64,
    pub n1_rpm: f64,
    pub n2_rpm: f64,
    pub n1_corrected_pct: f64,
    pub n2_corrected_pct: f64,
    /// FADEC N1 target (actual %) and the current N1 limit / idle schedule
    pub n1_command_pct: f64,
    pub n1_limit_pct: f64,
    pub n1_idle_pct: f64,
    /// What is limiting takeoff N1: "N1" (speed limit) or "EGT" (flat rating)
    pub rating_limit: String,
    pub n1_dot_pct_s: f64,
    pub n2_dot_pct_s: f64,
    /// Indicated EGT (thermocouple with lag), C. See `egt_valid`.
    pub egt_c: f64,
    pub egt_valid: bool,
    /// True gas temperature at the EGT probe, C
    pub egt_gas_c: f64,
    pub t4_k: f64,
    /// Steady-state T4 for the current speeds (what T4 would be with no transient)
    pub t4_steady_k: f64,
    /// Indicated fuel flow (kg/s and lb/h)
    pub fuel_flow_kg_s: f64,
    pub fuel_flow_pph: f64,
    /// Steady-state fuel flow for the current speeds
    pub fuel_flow_steady_kg_s: f64,
    /// FADEC fuel demand before and after schedule limiting (kg/s)
    pub fuel_command_kg_s: f64,
    pub fuel_accel_limit_kg_s: f64,
    pub fuel_decel_limit_kg_s: f64,
    /// Wf/Ps3 ratio in (kg/h)/kPa, the FADEC's schedule variable
    pub wf_p3_ratio: f64,
    pub ps3_kpa: f64,
    pub fuel_used_kg: f64,
    pub thrust_n: f64,
    pub thrust_lbf: f64,
    pub thrust_reverse_n: f64,
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
    pub bleed_fraction: f64,
    pub vbv_open: f64,
    pub vsv_angle_deg: f64,
    pub reverser_position: f64,
    pub hpc_surge_margin_pct: f64,
    pub fan_surge_margin_pct: f64,
    pub surge: bool,
    pub surge_count: u32,
    pub flameout_count: u32,
    pub fire_warning: bool,
    pub seized: bool,
    pub oil_starved_s: f64,
    pub windmill_n2_pct: f64,
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
    pub faults: Faults,
    pub logger: DataLogger,
    time_s: f64,
    mode: Mode,
    n1: f64,
    n2: f64,
    n1_dot: f64,
    n2_dot: f64,
    n1_cmd: f64,
    n1_limit: f64,
    n1_idle: f64,
    rating_limit: String,
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
    reverser_pos: f64,
    surge_timer: f64,
    surge_recovery_s: f64,
    surge_count: u32,
    flameout_count: u32,
    hung_timer: f64,
    bottle_msg_timer: f64,
    sm_hpc: f64,
    sm_fan: f64,
    takeoff_timer_s: f64,
    run_time_s: f64,
    rng: Rng,
    idle_fuel_kg_s: f64,
    egt_rated_k: f64,
    limit_cache_key: (i64, i64, i64, bool),
    last_cycle: CycleResult,
    warnings: Vec<String>,
    max_substep_s: f64,
}

pub const LOG_COLUMNS: &[&str] = &[
    "time_s", "mode", "tla", "fuel_lever", "starter", "anti_ice", "pack_bleed", "reverser",
    "altitude_m", "mach", "delta_isa_k", "tt2_k", "pt2_pa",
    "n1_pct", "n2_pct", "n1_corrected_pct", "n2_corrected_pct", "n1_command_pct", "n1_limit_pct",
    "egt_c", "egt_gas_c", "t4_k", "t4_steady_k",
    "fuel_flow_kg_s", "fuel_flow_pph", "fuel_flow_steady_kg_s", "fuel_command_kg_s", "fuel_accel_limit_kg_s", "fuel_decel_limit_kg_s", "wf_p3_ratio", "ps3_kpa", "fuel_used_kg",
    "thrust_n", "thrust_lbf", "thrust_reverse_n", "tsfc_lb_lbf_h",
    "oil_pressure_psi", "oil_temperature_c", "oil_quantity_qt", "vib_n1", "vib_n2",
    "bleed_fraction", "vbv_open", "vsv_angle_deg", "reverser_position", "hpc_surge_margin_pct", "fan_surge_margin_pct", "surge", "lit", "fire",
    "w2_kg_s", "w25_kg_s", "bypass_ratio", "fan_pr", "booster_pr", "hpc_pr", "opr",
    "tt13_k", "tt25_k", "tt3_k", "pt3_pa", "tt45_k", "tt5_k", "pt5_pa", "v9_m_s", "v19_m_s",
    "thermal_eff", "propulsive_eff", "overall_eff",
];

const TAU: f64 = std::f64::consts::TAU;

impl Engine {
    pub fn new(spec: EngineSpec) -> Self {
        let sizing = cycle::size_design_point(&spec);
        let a9 = sizing.core_nozzle_area_m2;
        let env = Environment::default();
        let inl = inlet_conditions(isa(env.altitude_m, env.delta_isa_k), env.mach);
        let cold = cycle(&spec, &inl, CycleInput { combustion: false, ..CycleInput::steady(0.0, 0.0, inl.tt2_k, Some(a9)) });
        let columns = LOG_COLUMNS.iter().map(|s| s.to_string()).collect();
        let idle_fuel_kg_s = {
            let n1c = spec.fadec.idle_n1_ground_pct / 100.0;
            let n2c = interp(&spec.spools.n2_vs_n1, spec.fadec.idle_n1_ground_pct) / 100.0;
            cycle::steady_point(&spec, &inl, CycleInput::steady(n1c, n2c, 0.0, None), a9).wf_kg_s
        };
        // Rated EGT: takeoff at the flat-rating corner point (SL, ISA + delta, static)
        let egt_rated_k = {
            let inl_c = inlet_conditions(isa(0.0, spec.rating.flat_rating_delta_isa_k), 0.0);
            let n1c = 1.0;
            let n2c = interp(&spec.spools.n2_vs_n1, 100.0) / 100.0;
            cycle::steady_point(&spec, &inl_c, CycleInput::steady(n1c, n2c, 0.0, None), a9).egt_k
        };
        let full_qt = spec.oil.full_qt;
        Engine {
            spec,
            sizing,
            controls: Controls::default(),
            environment: env,
            faults: Faults::none(),
            logger: DataLogger::new(10.0, columns),
            time_s: 0.0,
            mode: Mode::Off,
            n1: 0.0,
            n2: 0.0,
            n1_dot: 0.0,
            n2_dot: 0.0,
            n1_cmd: 0.0,
            n1_limit: 100.0,
            n1_idle: 21.0,
            rating_limit: "N1".into(),
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
            egt_gas_k: inl.tt2_k,
            egt_sensed_k: inl.tt2_k,
            ff_sensed: 0.0,
            ff_steady: 0.0,
            fuel_used_kg: 0.0,
            oil_temp_k: inl.tt2_k,
            oil_press_psi: 0.0,
            oil_qty_qt: full_qt,
            oil_starved_s: 0.0,
            seized: false,
            reverser_pos: 0.0,
            surge_timer: 0.0,
            surge_recovery_s: 0.0,
            surge_count: 0,
            flameout_count: 0,
            hung_timer: 0.0,
            bottle_msg_timer: 0.0,
            sm_hpc: 0.0,
            sm_fan: 0.0,
            takeoff_timer_s: 0.0,
            run_time_s: 0.0,
            rng: Rng(0x9E3779B97F4A7C15),
            idle_fuel_kg_s,
            egt_rated_k,
            limit_cache_key: (i64::MIN, 0, 0, false),
            last_cycle: cold,
            warnings: Vec::new(),
            max_substep_s: 0.02,
        }
    }

    /// Reset to cold and dark, keeping the spec, controls, environment and faults.
    pub fn reset(&mut self) {
        let spec = self.spec.clone();
        let (controls, env, faults, rate) = (self.controls, self.environment, self.faults, self.logger.rate_hz);
        *self = Engine::new(spec);
        self.controls = controls;
        self.environment = env;
        self.faults = faults;
        self.logger.set_rate(rate);
    }

    /// Replace the engine specification (re-sizes the design point) and reset.
    pub fn set_spec(&mut self, spec: EngineSpec) {
        let (controls, env, rate) = (self.controls, self.environment, self.logger.rate_hz);
        *self = Engine::new(spec);
        self.controls = controls;
        self.environment = env;
        self.logger.set_rate(rate);
    }

    /// Jump straight to a stabilised running condition.
    pub fn set_running(&mut self, tla: f64) {
        self.controls.fuel_lever = true;
        self.controls.starter = false;
        self.controls.tla = tla.clamp(0.0, 1.0);
        self.seized = false;
        let inl = self.inlet();
        self.update_limits(&inl);
        let n1c = self.n1_command_corrected(&inl);
        let st = inl.theta.sqrt();
        self.n1 = n1c * st;
        self.n2 = interp(&self.spec.spools.n2_vs_n1, n1c) * st;
        self.n1_cmd = self.n1;
        self.mode = Mode::Running;
        self.lit = true;
        self.n1_dot = 0.0;
        self.n2_dot = 0.0;
        let base = self.base_input(&inl);
        let r = cycle::steady_point(&self.spec, &inl, base, self.sizing.core_nozzle_area_m2);
        self.t4 = r.t4_k;
        self.t4_ss = r.t4_k;
        self.wf = r.wf_kg_s;
        self.wf_cmd = r.wf_kg_s;
        self.ps3_kpa = r.ps3_kpa;
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

    fn bleed_fraction(&self) -> f64 {
        let b = &self.spec.bleed;
        (if self.controls.anti_ice { b.anti_ice_fraction } else { 0.0 }) + (if self.controls.pack_bleed { b.pack_fraction } else { 0.0 })
    }

    fn base_input(&self, inl: &Inlet) -> CycleInput {
        let st = inl.theta.sqrt();
        let n2c_pct = self.n2 / st;
        CycleInput {
            n1c: self.n1 / st / 100.0,
            n2c: n2c_pct / 100.0,
            t4_k: 0.0,
            a9_m2: Some(self.sizing.core_nozzle_area_m2),
            combustion: true,
            bleed_fraction: self.bleed_fraction(),
            vbv_open: interp(&self.spec.bleed.vbv_open_vs_n2c, n2c_pct),
            reverser: self.reverser_pos,
            reverser_efficiency: self.spec.reverser.efficiency,
        }
    }

    /// Idle N1 schedule (corrected %).
    fn idle_n1_corrected(&self) -> f64 {
        let f = &self.spec.fadec;
        let airborne = self.environment.mach > 0.15;
        let base = if airborne { f.idle_n1_flight_pct } else { f.idle_n1_ground_pct };
        let alt = base + f.idle_n1_per_km * (self.environment.altitude_m / 1000.0).max(0.0);
        alt + if self.controls.anti_ice { f.idle_n1_anti_ice_pct } else { 0.0 }
    }

    /// Recompute the N1 limit (flat rating / overspeed / reverse) when the
    /// environment changes. Cached because the EGT-limited search costs a few
    /// hundred cycle evaluations.
    fn update_limits(&mut self, inl: &Inlet) {
        let key = (
            (self.environment.altitude_m / 50.0).round() as i64,
            (self.environment.mach * 100.0).round() as i64,
            (self.environment.delta_isa_k * 2.0).round() as i64,
            self.controls.anti_ice || self.controls.pack_bleed,
        );
        self.n1_idle = self.idle_n1_corrected() * inl.theta.sqrt();
        if key == self.limit_cache_key {
            return;
        }
        self.limit_cache_key = key;
        let st = inl.theta.sqrt();
        let speed_limit_pct = self.spec.limits.n1_max_pct;
        // Takeoff rating: 100% corrected, capped by the actual speed limit
        let mut n1c_max = (100.0f64).min(speed_limit_pct / st);
        let mut limit = "N1".to_string();
        if self.spec.fadec.egt_flat_rating {
            let corner = isa(self.environment.altitude_m, 0.0).temperature_k + self.spec.rating.flat_rating_delta_isa_k;
            if inl.ambient.temperature_k > corner + 0.05 {
                // Hold rated EGT: find the corrected N1 at which steady EGT equals the rated value
                let a9 = self.sizing.core_nozzle_area_m2;
                let bleed = self.bleed_fraction();
                let egt_at = |n1c_pct: f64| {
                    let n2c = interp(&self.spec.spools.n2_vs_n1, n1c_pct) / 100.0;
                    let base = CycleInput { bleed_fraction: bleed, ..CycleInput::steady(n1c_pct / 100.0, n2c, 0.0, Some(a9)) };
                    cycle::steady_point(&self.spec, inl, base, a9).egt_k
                };
                if egt_at(n1c_max) > self.egt_rated_k {
                    let (mut lo, mut hi) = (70.0, n1c_max);
                    for _ in 0..24 {
                        let mid = 0.5 * (lo + hi);
                        if egt_at(mid) > self.egt_rated_k { hi = mid } else { lo = mid }
                    }
                    n1c_max = 0.5 * (lo + hi);
                    limit = "EGT".into();
                }
            }
        }
        self.n1_limit = n1c_max * st;
        self.rating_limit = limit;
    }

    /// FADEC N1 command (corrected %).
    fn n1_command_corrected(&self, inl: &Inlet) -> f64 {
        let f = &self.spec.fadec;
        let st = inl.theta.sqrt();
        let tla = self.controls.tla.clamp(0.0, 1.0);
        let idle_c = self.n1_idle / st;
        let mut max_c = self.n1_limit / st;
        if self.reverser_pos > 0.5 {
            max_c = max_c.min(self.spec.reverser.max_n1_pct / st);
        }
        if self.faults.n1_governor_fail {
            // Overspeed protection lost: the schedule runs past the limit
            max_c += 8.0;
        }
        if self.controls.n1_demand_pct >= 0.0 {
            return (self.controls.n1_demand_pct / st).clamp(idle_c, max_c);
        }
        (idle_c + (max_c - idle_c) * tla.powf(f.tla_exponent)).max(idle_c)
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
        let st = inl.theta.sqrt();
        let spec = self.spec.clone();
        let s = &spec.spools;
        let sp = &spec.start;
        let fd = &spec.fadec;
        let a9 = self.sizing.core_nozzle_area_m2;
        let c = self.controls;
        let f = self.faults;
        self.warnings.clear();
        self.update_limits(&inl);

        // One-shot triggers
        self.faults.trigger_surge = false;
        self.faults.trigger_flameout = false;
        self.faults.fire_bottle = false;

        // Windmilling floors in flight
        let n1_wm = (sp.windmill_n1_per_mach * self.environment.mach).clamp(0.0, 35.0);
        let n2_wm = (sp.windmill_n2_gain * self.environment.mach.max(0.0).powf(sp.windmill_n2_exp)).clamp(0.0, 40.0);

        let starter_ok = c.starter && !f.starter_inop && !self.seized;
        let starter_max = if f.starter_weak { 18.0 } else { sp.starter_max_n2_pct };
        let starter_cutout = if f.starter_early_cutout { 30.0 } else { sp.starter_cutout_n2_pct };
        let fuel_available = c.fuel_lever && !f.fuel_pump_fail;
        let light_possible = self.n2 >= sp.min_light_n2_pct && self.environment.altitude_m <= sp.max_light_altitude_m && !f.ignition_fail;

        // ---- Mode transitions ---------------------------------------------
        match self.mode {
            Mode::Off => {
                if starter_ok {
                    self.mode = Mode::Motoring;
                } else if fuel_available && self.n2 >= sp.min_light_n2_pct {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                }
            }
            Mode::Motoring => {
                if fuel_available && self.n2 >= sp.min_light_n2_pct {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                } else if !starter_ok {
                    self.mode = Mode::Off;
                }
            }
            Mode::Starting => {
                if !fuel_available || self.seized {
                    self.mode = Mode::Shutdown;
                    self.lit = false;
                    self.fuel_on_since = None;
                } else if self.lit && self.n2 >= s.n2_idle_pct * st - 0.3 {
                    self.mode = Mode::Running;
                    self.n1_cmd = self.n1;
                    self.hung_timer = 0.0;
                }
            }
            Mode::Running => {
                if !fuel_available || self.seized {
                    self.mode = Mode::Shutdown;
                    self.lit = false;
                    self.fuel_on_since = None;
                } else if !self.lit {
                    // Flameout: FADEC auto-relight (ignition on, sub-idle schedule)
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                } else if self.n2 < s.n2_idle_pct * st - 8.0 {
                    // Dropped well below idle (surge cycling, severe over-bleed):
                    // the FADEC falls back to its sub-idle (start) fuel schedule.
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s - sp.light_off_delay_s);
                    self.surge_timer = 0.0;
                }
            }
            Mode::Shutdown => {
                if fuel_available && self.n2 >= sp.min_light_n2_pct && !self.seized {
                    self.mode = Mode::Starting;
                    self.fuel_on_since = Some(self.time_s);
                } else if starter_ok && self.n2 < starter_max {
                    self.mode = Mode::Motoring;
                } else if self.n2 < 1.0 {
                    self.mode = Mode::Off;
                }
            }
        }

        self.starter_engaged = starter_ok && self.n2 < starter_cutout && matches!(self.mode, Mode::Motoring | Mode::Starting | Mode::Off | Mode::Shutdown);

        // Starter torque falls linearly to zero at ~65% N2; it never retards.
        let starter_rate0 = starter_max / sp.starter_tau_s; // %/s at zero speed
        let starter_assist = if self.starter_engaged { (starter_rate0 * (1.0 - self.n2 / (starter_max * 2.33))).max(0.0) } else { 0.0 };
        // Rotor drag (friction + compressor work with no combustion): decay toward windmill
        let drag_rate = |n2: f64| (n2 - n2_wm).max(0.0) / 12.0;

        // ---- Reverser -------------------------------------------------------
        if !f.reverser_stuck {
            let target = if c.reverser { 1.0 } else { 0.0 };
            let rate = h / spec.reverser.deploy_time_s;
            self.reverser_pos += (target - self.reverser_pos).clamp(-rate, rate);
        }

        // ---- Spool dynamics -----------------------------------------------
        let n1_prev = self.n1;
        let n2_prev = self.n2;
        let combustion;
        let mut wf_dyn = 0.0;

        match self.mode {
            Mode::Off | Mode::Motoring | Mode::Shutdown => {
                combustion = false;
                if self.seized {
                    self.n2 += (0.0 - self.n2) / 4.0 * h;
                } else if self.starter_engaged {
                    self.n2 += (starter_assist - drag_rate(self.n2)) * h;
                } else {
                    let tau = if self.mode == Mode::Shutdown { s.n2_spooldown_tau_s } else { 12.0 };
                    self.n2 += (n2_wm - self.n2) / tau * h;
                }
                let n1_target = if self.starter_engaged { (sp.motoring_n1_over_n2 * self.n2).max(n1_wm) } else { n1_wm };
                let tau = if self.mode == Mode::Shutdown || self.n1 > n1_target { s.n1_spooldown_tau_s } else { 8.0 };
                self.n1 += (n1_target - self.n1) / tau * h;
                self.t4 = inl.tt2_k;
                self.t4_ss = inl.tt2_k;
                self.wf = 0.0;
                self.wf_cmd = 0.0;
                self.wf_max = 0.0;
                self.wf_min = 0.0;
            }
            Mode::Starting => {
                // Ignition is on; light-off after the delay if conditions allow
                if let Some(t0) = self.fuel_on_since {
                    if !self.lit && self.time_s - t0 >= sp.light_off_delay_s && light_possible {
                        self.lit = true;
                    }
                    if !self.lit && self.time_s - t0 > sp.light_off_delay_s + 10.0 {
                        if f.ignition_fail {
                            self.warnings.push("WET START: fuel flowing, no light-off (igniters)".into());
                        } else if self.n2 < sp.min_light_n2_pct {
                            self.warnings.push(format!("NO LIGHT: N2 {:.0}% below {:.0}% (windmill too slow, use starter)", self.n2, sp.min_light_n2_pct));
                        } else if self.environment.altitude_m > sp.max_light_altitude_m {
                            self.warnings.push("NO LIGHT: above relight altitude envelope".into());
                        }
                    }
                }
                combustion = self.lit;
                // Start fuel schedule (scaled to meet the governed idle fuel flow)
                let sched_idle = interp(&sp.start_fuel_schedule, s.n2_idle_pct).max(1e-6);
                let scale = self.idle_fuel_kg_s * inl.delta / sched_idle * f.start_fuel_factor.max(0.1);
                wf_dyn = interp(&sp.start_fuel_schedule, self.n2 / st) * scale;
                self.wf = wf_dyn;
                self.wf_cmd = wf_dyn;
                let mut n2_dot = starter_assist - drag_rate(self.n2);
                if self.lit {
                    // Gross turbine acceleration from the sub-idle schedule, scaled with fuel
                    n2_dot += interp(&sp.start_accel_vs_n2, self.n2 / st) * (0.4 + 0.6 * f.start_fuel_factor).max(0.2);
                }
                self.n2 += n2_dot * h;
                if self.lit && self.n2 < sp.min_light_n2_pct - 3.0 {
                    // Airflow collapsed: the flame cannot be sustained
                    self.lit = false;
                    self.fuel_on_since = Some(self.time_s);
                }
                // Hung start: lit, starter dropped out, not accelerating, below idle
                if self.lit && !self.starter_engaged && self.n2 < s.n2_idle_pct * st - 3.0 {
                    if n2_dot < 0.15 { self.hung_timer += h } else { self.hung_timer = 0.0 }
                    if self.hung_timer > 6.0 {
                        self.warnings.push(format!("HUNG START: N2 stalled at {:.0}%", self.n2));
                    }
                }
                let ratio = if self.lit { sp.lit_n1_over_n2 } else { sp.motoring_n1_over_n2 };
                let n1_target = (ratio * self.n2).max(n1_wm);
                self.n1 += (n1_target - self.n1) / 2.5 * h;
            }
            Mode::Running => {
                combustion = true;
                // --- FADEC: N1 governing through fuel flow ---
                let n1c_cmd = self.n1_command_corrected(&inl);
                self.n1_cmd = n1c_cmd * st;
                let base = self.base_input(&inl);
                let ss = cycle::steady_point(&spec, &inl, base, a9);
                self.t4_ss = ss.t4_k;
                self.ff_steady = ss.wf_kg_s;
                self.ps3_kpa = ss.ps3_kpa;
                // Feed-forward of the steady fuel plus proportional N1 governor
                let demand = ss.wf_kg_s + fd.governor_gain_kg_s_per_pct * (self.n1_cmd - self.n1);
                let n2c_pct = self.n2 / st;
                self.wf_max = interp(&fd.accel_wf_p3_vs_n2c, n2c_pct) * self.ps3_kpa / 3600.0;
                // The decel floor protects against lean blow-out; it can never ask
                // for more than the steady fuel (that would be over-fuelling).
                self.wf_min = (interp(&fd.decel_wf_p3_vs_n2c, n2c_pct) * self.ps3_kpa / 3600.0).max(fd.min_fuel_kg_s.min(ss.wf_kg_s));
                // After a surge the FADEC re-arms the accel schedule gradually
                // (no slam re-acceleration into a stalled compressor).
                if self.surge_recovery_s > 0.0 {
                    self.surge_recovery_s -= h;
                    let f = 1.0 - self.surge_recovery_s / 4.0;
                    self.wf_max = ss.wf_kg_s + (self.wf_max - ss.wf_kg_s) * f.clamp(0.0, 1.0);
                }
                self.wf_cmd = demand;
                let mut target = demand.clamp(self.wf_min, self.wf_max.max(self.wf_min));
                // Surge recovery: the FADEC pulls fuel to the deceleration floor
                if self.surge_timer > 0.0 {
                    target = self.wf_min;
                }
                self.wf += (target - self.wf) / fd.fuel_lag_s * h;
                wf_dyn = self.wf;
                // Combustor: actual T4 from the actual fuel flow
                let cs = cycle::cold_section(&spec, &inl, base.n1c, base.n2c);
                let w_b = cs.w25 * (1.0 - spec.design_point.cooling_bleed_fraction - base.bleed_fraction);
                self.t4 = cycle::t4_for_fuel(&spec, w_b, cs.tt3, self.wf);
                // --- LP spool: integrates the LPT / fan power imbalance ---
                // With choked turbines the turbine power scales with T4 at fixed speeds.
                let ratio = (self.t4 / self.t4_ss).max(0.2);
                let p_excess = ss.lpt_power_w * (ratio - 1.0);
                let w1 = (self.n1 / 100.0 * spec.design_point.n1_100pct_rpm * TAU / 60.0).max(5.0);
                let mut a1 = p_excess / (s.lp_inertia_kg_m2 * w1);
                if self.surge_timer > 0.0 {
                    a1 -= 0.06 * w1; // loss of turbine work during the surge
                }
                self.n1 += a1 / (spec.design_point.n1_100pct_rpm * TAU / 60.0) * 100.0 * h;
                self.n1 = self.n1.max(n1_wm);
                // --- HP spool: follows its operating line with a lag, faster when over-fuelled ---
                let n2_ss = interp(&s.n2_vs_n1, self.n1 / st) * st;
                let lag = s.n2_lag_s / ratio.clamp(0.5, 2.0);
                self.n2 += (n2_ss - self.n2) / lag * h;
                if self.surge_timer > 0.0 {
                    self.n2 -= 3.0 * h;
                }
                // --- Surge margin (HPC operating line rises with over-fuelling) ---
                let sm_hpc_avail = spec.bleed.hpc_surge_margin_pct * (0.55 + 0.45 * base.n2c.min(1.0)) - f.compressor_damage_pct - if f.fod { 4.0 } else { 0.0 };
                // Back-pressure from the choked HPT raises P3 roughly as T4^0.3
                // once the HPC flow re-adjusts along its speed line.
                self.sm_hpc = ((1.0 + sm_hpc_avail / 100.0) / ratio.powf(0.3) - 1.0) * 100.0;
                self.sm_fan = spec.bleed.fan_surge_margin_pct * (0.6 + 0.4 * base.n1c.min(1.0)) - if f.fod { 12.0 } else { 0.0 };
                if self.surge_timer <= 0.0 && (self.sm_hpc < 0.0 || f.trigger_surge) {
                    self.surge_timer = 1.5;
                    self.surge_recovery_s = 4.0;
                    self.surge_count += 1;
                }
                // --- Flameout ---
                if f.trigger_flameout || self.wf < 0.5 * fd.min_fuel_kg_s {
                    self.lit = false;
                    self.flameout_count += 1;
                }
            }
        }
        if self.surge_timer > 0.0 {
            self.surge_timer -= h;
            self.warnings.push("ENGINE SURGE / STALL".into());
        }
        self.n1 = self.n1.max(0.0);
        self.n2 = self.n2.max(0.0);
        self.n1_dot = (self.n1 - n1_prev) / h;
        self.n2_dot = (self.n2 - n2_prev) / h;

        // ---- Thermodynamic cycle at the actual operating point --------------
        let base = self.base_input(&inl);
        let r = if combustion {
            if self.mode == Mode::Starting {
                let cs = cycle::cold_section(&spec, &inl, base.n1c, base.n2c);
                let w_b = cs.w25 * (1.0 - spec.design_point.cooling_bleed_fraction - base.bleed_fraction);
                self.t4 = cycle::t4_for_fuel(&spec, w_b, cs.tt3, wf_dyn);
                self.t4_ss = self.t4;
                self.ff_steady = wf_dyn;
            }
            let mut r = cycle(&spec, &inl, CycleInput { t4_k: self.t4, ..base });
            if self.surge_timer > 0.0 {
                r.net_thrust_n *= 0.3;
                r.egt_k += 150.0;
            }
            if f.fod {
                r.egt_k += 20.0;
            }
            r
        } else {
            self.ff_steady = 0.0;
            cycle(&spec, &inl, CycleInput { t4_k: inl.tt2_k, combustion: false, ..base })
        };

        // ---- Gas temperature at the EGT probe ------------------------------
        if combustion {
            self.egt_gas_k += (r.egt_k - self.egt_gas_k) / 0.35 * h;
        } else {
            let tau = 25.0 + 60.0 / (1.0 + self.n2 / 10.0);
            self.egt_gas_k += (inl.tt2_k - self.egt_gas_k) / tau * h;
        }

        // ---- Sensors --------------------------------------------------------
        self.egt_sensed_k += (self.egt_gas_k - self.egt_sensed_k) / 1.5 * h;
        self.ff_sensed += (wf_dyn - self.ff_sensed) / 0.4 * h;
        self.fuel_used_kg += wf_dyn * h;

        // ---- Oil system -----------------------------------------------------
        let o = &spec.oil;
        self.oil_qty_qt = (self.oil_qty_qt - f.oil_leak_qt_per_min / 60.0 * h).max(0.0);
        let mut oil_target = self.oil_temp_target(&inl);
        if f.fire && (self.mode == Mode::Running || self.mode == Mode::Starting) {
            oil_target += 80.0;
        }
        if self.oil_qty_qt < o.starvation_qt {
            oil_target += 40.0 * (1.0 - self.oil_qty_qt / o.starvation_qt);
        }
        self.oil_temp_k += (oil_target - self.oil_temp_k) / o.temp_tau_s * h;
        let oil_temp_c = self.oil_temp_k - 273.15;
        let mut press = o.pressure_gain_psi_per_pct * self.n2 + o.pressure_offset_psi - o.pressure_temp_derate_psi_per_k * (oil_temp_c - 90.0).max(0.0);
        if self.oil_qty_qt < o.starvation_qt {
            press *= self.oil_qty_qt / o.starvation_qt;
        }
        self.oil_press_psi = press.max(0.0) + 0.15 * self.rng.gauss() * (self.n2 > 5.0) as u8 as f64;
        if matches!(self.mode, Mode::Running | Mode::Starting) && self.oil_press_psi < spec.limits.oil_pressure_min_psi {
            self.oil_starved_s += h;
        } else {
            self.oil_starved_s = (self.oil_starved_s - h).max(0.0);
        }
        if self.oil_starved_s > o.seizure_time_s && !self.seized {
            self.seized = true;
            self.lit = false;
        }

        // Fire bottle: extinguishes only once the fuel is shut off
        if f.fire_bottle {
            if !c.fuel_lever {
                self.faults.fire = false;
                self.bottle_msg_timer = 0.0;
            } else {
                self.bottle_msg_timer = 5.0;
            }
        }

        // ---- Timers ---------------------------------------------------------
        if self.mode == Mode::Running {
            self.run_time_s += h;
            if self.controls.tla > 0.9 && self.reverser_pos < 0.5 {
                self.takeoff_timer_s += h;
            } else {
                self.takeoff_timer_s = (self.takeoff_timer_s - h).max(0.0);
            }
        }
        self.time_s += h;

        // ---- Limit monitoring -----------------------------------------------
        let l = &spec.limits;
        let egt_c = self.egt_sensed_k - 273.15;
        if self.faults.fire {
            self.warnings.push("ENGINE FIRE".into());
        }
        if self.seized {
            self.warnings.push("ENGINE SEIZED (bearing failure after oil starvation)".into());
        } else if self.oil_starved_s > 5.0 {
            self.warnings.push(format!("OIL STARVATION {:.0} s: bearing failure in {:.0} s", self.oil_starved_s, o.seizure_time_s - self.oil_starved_s));
        }
        if f.egt_probe_fail {
            self.warnings.push("EGT INDICATION FAILED".into());
        }
        if self.mode == Mode::Starting && egt_c > l.egt_start_c {
            self.warnings.push(format!("HOT START: EGT {:.0} C exceeds start limit {:.0} C", egt_c, l.egt_start_c));
        }
        if self.mode == Mode::Running {
            if egt_c > l.egt_takeoff_c {
                self.warnings.push(format!("EGT {:.0} C exceeds takeoff limit {:.0} C", egt_c, l.egt_takeoff_c));
            } else if egt_c > l.egt_max_continuous_c && self.takeoff_timer_s > l.takeoff_time_limit_s {
                self.warnings.push(format!("EGT above max continuous {:.0} C for more than {:.0} s", l.egt_max_continuous_c, l.takeoff_time_limit_s));
            }
            if self.n1 > l.n1_max_pct {
                self.warnings.push(format!("N1 OVERSPEED {:.1}% (limit {:.1}%)", self.n1, l.n1_max_pct));
            }
            if self.n2 > l.n2_max_pct {
                self.warnings.push(format!("N2 OVERSPEED {:.1}% (limit {:.1}%)", self.n2, l.n2_max_pct));
            }
            if self.oil_press_psi < l.oil_pressure_min_psi {
                self.warnings.push(format!("LOW OIL PRESSURE {:.0} psi (min {:.0})", self.oil_press_psi, l.oil_pressure_min_psi));
            }
            if oil_temp_c > l.oil_temp_max_continuous_c {
                self.warnings.push(format!("HIGH OIL TEMPERATURE {:.0} C (max {:.0})", oil_temp_c, l.oil_temp_max_continuous_c));
            }
            if self.sm_hpc < 5.0 && self.surge_timer <= 0.0 {
                self.warnings.push(format!("HPC surge margin {:.1}%", self.sm_hpc));
            }
            if self.reverser_pos > 0.05 && self.environment.mach > 0.3 && self.environment.altitude_m > 100.0 {
                self.warnings.push("REVERSER DEPLOYED IN FLIGHT".into());
            }
        }
        if self.oil_qty_qt < 4.0 {
            self.warnings.push(format!("LOW OIL QUANTITY {:.1} qt", self.oil_qty_qt));
        }
        if self.mode == Mode::Starting && !self.lit && self.flameout_count > 0 {
            self.warnings.push("FLAMEOUT: auto-relight in progress".into());
        }
        if matches!(self.mode, Mode::Off | Mode::Shutdown | Mode::Motoring) && c.fuel_lever && !f.fuel_pump_fail && self.n2 < sp.min_light_n2_pct {
            self.warnings.push(format!("NO LIGHT: N2 {:.0}% below {:.0}% (windmill too slow, use starter)", self.n2, sp.min_light_n2_pct));
        }
        if self.bottle_msg_timer > 0.0 {
            self.bottle_msg_timer -= h;
            self.warnings.push("FIRE BOTTLE DISCHARGED: fire persists, fuel lever still RUN".into());
        }

        self.last_cycle = r;

        // ---- Logging ---------------------------------------------------------
        if self.logger.due(self.time_s) {
            let state = self.state_inner(&inl);
            self.logger.maybe_sample(self.time_s, || log_row(&state));
        }
    }

    pub fn state(&self) -> EngineState {
        let inl = self.inlet();
        self.state_inner(&inl)
    }

    fn state_inner(&self, inl: &Inlet) -> EngineState {
        let d = &self.spec.design_point;
        let st = inl.theta.sqrt();
        let r = &self.last_cycle;
        let thrust = r.net_thrust_n;
        let tsfc = if thrust > 100.0 { self.ff_sensed / thrust * 3600.0 * 9.80665 } else { 0.0 };
        let o = &self.spec.oil;
        let n1f = self.n1 / 100.0;
        let n2f = self.n2 / 100.0;
        let mut rng = self.rng.clone();
        let fod = if self.faults.fod { 2.2 } else { 0.0 };
        let seized = if self.seized { 3.0 } else { 0.0 };
        let starve = (self.oil_starved_s / self.spec.oil.seizure_time_s).clamp(0.0, 1.0) * 2.5;
        let surge = if self.surge_timer > 0.0 { 2.0 } else { 0.0 };
        let vib_n1 = if self.n1 > 2.0 { (0.3 + 1.1 * n1f * n1f + fod + surge + 0.08 * rng.gauss()).clamp(0.0, 5.0) } else { 0.0 };
        let vib_n2 = if self.n2 > 2.0 { (0.2 + 0.7 * n2f * n2f + starve + seized + surge + 0.06 * rng.gauss()).clamp(0.0, 5.0) } else { 0.0 };
        let sp = &self.spec.start;
        EngineState {
            time_s: self.time_s,
            mode: self.mode,
            controls: self.controls,
            environment: self.environment,
            faults: self.faults,
            inlet: *inl,
            n1_pct: self.n1,
            n2_pct: self.n2,
            n1_rpm: n1f * d.n1_100pct_rpm,
            n2_rpm: n2f * d.n2_100pct_rpm,
            n1_corrected_pct: self.n1 / st,
            n2_corrected_pct: self.n2 / st,
            n1_command_pct: self.n1_cmd,
            n1_limit_pct: self.n1_limit,
            n1_idle_pct: self.n1_idle,
            rating_limit: self.rating_limit.clone(),
            n1_dot_pct_s: self.n1_dot,
            n2_dot_pct_s: self.n2_dot,
            egt_c: if self.faults.egt_probe_fail { 0.0 } else { self.egt_sensed_k - 273.15 },
            egt_valid: !self.faults.egt_probe_fail,
            egt_gas_c: self.egt_gas_k - 273.15,
            t4_k: self.t4,
            t4_steady_k: self.t4_ss,
            fuel_flow_kg_s: self.ff_sensed,
            fuel_flow_pph: self.ff_sensed * 7936.64,
            fuel_flow_steady_kg_s: self.ff_steady,
            fuel_command_kg_s: self.wf_cmd,
            fuel_accel_limit_kg_s: self.wf_max,
            fuel_decel_limit_kg_s: self.wf_min,
            wf_p3_ratio: if self.ps3_kpa > 1.0 { self.wf * 3600.0 / self.ps3_kpa } else { 0.0 },
            ps3_kpa: self.ps3_kpa,
            fuel_used_kg: self.fuel_used_kg,
            thrust_n: thrust,
            thrust_lbf: thrust / 4.448222,
            thrust_reverse_n: r.thrust_reverse_n,
            tsfc_lb_lbf_h: tsfc,
            oil_pressure_psi: self.oil_press_psi,
            oil_temperature_c: self.oil_temp_k - 273.15,
            oil_quantity_qt: (self.oil_qty_qt - o.gulp_qt * (self.n2 / self.spec.spools.n2_idle_pct).clamp(0.0, 1.0)).max(0.0),
            vib_n1,
            vib_n2,
            starter_engaged: self.starter_engaged,
            ignition: self.mode == Mode::Starting,
            lit: self.lit,
            bleed_fraction: self.bleed_fraction(),
            vbv_open: interp(&self.spec.bleed.vbv_open_vs_n2c, self.n2 / st),
            vsv_angle_deg: interp(&self.spec.bleed.vsv_angle_vs_n2c, self.n2 / st),
            reverser_position: self.reverser_pos,
            hpc_surge_margin_pct: self.sm_hpc,
            fan_surge_margin_pct: self.sm_fan,
            surge: self.surge_timer > 0.0,
            surge_count: self.surge_count,
            flameout_count: self.flameout_count,
            fire_warning: self.faults.fire,
            seized: self.seized,
            oil_starved_s: self.oil_starved_s,
            windmill_n2_pct: (sp.windmill_n2_gain * self.environment.mach.max(0.0).powf(sp.windmill_n2_exp)).clamp(0.0, 40.0),
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

fn b(v: bool) -> f64 {
    v as u8 as f64
}

/// One logger row, in the order of [`LOG_COLUMNS`].
pub fn log_row(s: &EngineState) -> Vec<f64> {
    let c = &s.cycle;
    let st = |id: &str| c.stations.iter().find(|x| x.id == id).cloned().unwrap_or_default();
    vec![
        s.time_s, mode_code(s.mode), s.controls.tla, b(s.controls.fuel_lever), b(s.controls.starter), b(s.controls.anti_ice), b(s.controls.pack_bleed), b(s.controls.reverser),
        s.environment.altitude_m, s.environment.mach, s.environment.delta_isa_k, s.inlet.tt2_k, s.inlet.pt2_pa,
        s.n1_pct, s.n2_pct, s.n1_corrected_pct, s.n2_corrected_pct, s.n1_command_pct, s.n1_limit_pct,
        s.egt_c, s.egt_gas_c, s.t4_k, s.t4_steady_k,
        s.fuel_flow_kg_s, s.fuel_flow_pph, s.fuel_flow_steady_kg_s, s.fuel_command_kg_s, s.fuel_accel_limit_kg_s, s.fuel_decel_limit_kg_s, s.wf_p3_ratio, s.ps3_kpa, s.fuel_used_kg,
        s.thrust_n, s.thrust_lbf, s.thrust_reverse_n, s.tsfc_lb_lbf_h,
        s.oil_pressure_psi, s.oil_temperature_c, s.oil_quantity_qt, s.vib_n1, s.vib_n2,
        s.bleed_fraction, s.vbv_open, s.vsv_angle_deg, s.reverser_position, s.hpc_surge_margin_pct, s.fan_surge_margin_pct, b(s.surge), b(s.lit), b(s.fire_warning),
        c.w2_kg_s, c.w25_kg_s, c.bypass_ratio, c.fan_pressure_ratio, c.booster_pressure_ratio, c.hpc_pressure_ratio, c.overall_pressure_ratio,
        st("13").tt_k, st("25").tt_k, st("3").tt_k, st("3").pt_pa, st("45").tt_k, st("5").tt_k, st("5").pt_pa,
        c.core_nozzle.exit_velocity_m_s, c.bypass_nozzle.exit_velocity_m_s,
        c.thermal_efficiency, c.propulsive_efficiency, c.overall_efficiency,
    ]
}
