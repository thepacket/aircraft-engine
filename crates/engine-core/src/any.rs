//! Engine-kind dispatcher: one interface over the turbofan, piston and
//! turboprop models, used by the WebAssembly bindings and the CLI.

use crate::engine::Engine;
use crate::logger::DataLogger;
use crate::piston::{PistonEngine, PistonSpec};
use crate::spec::EngineSpec;
use crate::turboprop::{TurbopropEngine, TurbopropSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnySpec {
    Turbofan(EngineSpec),
    Piston(PistonSpec),
    Turboprop(TurbopropSpec),
}

impl AnySpec {
    pub fn default_for(kind: &str) -> Option<Self> {
        match kind {
            "turbofan" | "cfm56" => Some(AnySpec::Turbofan(EngineSpec::cfm56_7b26())),
            "piston" | "o360" => Some(AnySpec::Piston(PistonSpec::lycoming_o360())),
            "turboprop" | "pt6" => Some(AnySpec::Turboprop(TurbopropSpec::pt6a_114a())),
            _ => None,
        }
    }

    /// Parse a spec JSON. A document without a `kind` tag is a turbofan spec
    /// (backwards compatible with the first release).
    pub fn from_json(s: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if v.get("kind").is_none() {
            return serde_json::from_value::<EngineSpec>(v).map(AnySpec::Turbofan).map_err(|e| e.to_string());
        }
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("spec serialises")
    }

    pub fn kind(&self) -> &'static str {
        match self {
            AnySpec::Turbofan(_) => "turbofan",
            AnySpec::Piston(_) => "piston",
            AnySpec::Turboprop(_) => "turboprop",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnyEngine {
    Turbofan(Engine),
    Piston(PistonEngine),
    Turboprop(TurbopropEngine),
}

impl AnyEngine {
    pub fn new(spec: AnySpec) -> Self {
        match spec {
            AnySpec::Turbofan(s) => AnyEngine::Turbofan(Engine::new(s)),
            AnySpec::Piston(s) => AnyEngine::Piston(PistonEngine::new(s)),
            AnySpec::Turboprop(s) => AnyEngine::Turboprop(TurbopropEngine::new(s)),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            AnyEngine::Turbofan(_) => "turbofan",
            AnyEngine::Piston(_) => "piston",
            AnyEngine::Turboprop(_) => "turboprop",
        }
    }

    pub fn spec(&self) -> AnySpec {
        match self {
            AnyEngine::Turbofan(e) => AnySpec::Turbofan(e.spec.clone()),
            AnyEngine::Piston(e) => AnySpec::Piston(e.spec.clone()),
            AnyEngine::Turboprop(e) => AnySpec::Turboprop(e.spec.clone()),
        }
    }

    pub fn sizing_json(&self) -> String {
        match self {
            AnyEngine::Turbofan(e) => serde_json::to_string(&e.sizing).unwrap_or_default(),
            AnyEngine::Piston(e) => serde_json::to_string(&serde_json::json!({ "rated_power_w": e.spec.rating.rated_power_w, "rated_rpm": e.spec.rating.rated_rpm })).unwrap_or_default(),
            AnyEngine::Turboprop(e) => serde_json::to_string(&e.sizing).unwrap_or_default(),
        }
    }

    pub fn reset(&mut self) {
        match self {
            AnyEngine::Turbofan(e) => e.reset(),
            AnyEngine::Piston(e) => e.reset(),
            AnyEngine::Turboprop(e) => e.reset(),
        }
    }

    pub fn step(&mut self, dt: f64) {
        match self {
            AnyEngine::Turbofan(e) => e.step(dt),
            AnyEngine::Piston(e) => e.step(dt),
            AnyEngine::Turboprop(e) => e.step(dt),
        }
    }

    pub fn time(&self) -> f64 {
        match self {
            AnyEngine::Turbofan(e) => e.time(),
            AnyEngine::Piston(e) => e.time(),
            AnyEngine::Turboprop(e) => e.time(),
        }
    }

    pub fn state_json(&self) -> String {
        match self {
            AnyEngine::Turbofan(e) => {
                // Tag the turbofan state with its kind like the others
                let mut v = serde_json::to_value(e.state()).unwrap_or_default();
                if let Some(o) = v.as_object_mut() { o.insert("kind".into(), "turbofan".into()); }
                v.to_string()
            }
            AnyEngine::Piston(e) => serde_json::to_string(&e.state()).unwrap_or_default(),
            AnyEngine::Turboprop(e) => serde_json::to_string(&e.state()).unwrap_or_default(),
        }
    }

    /// Jump to a stabilised running condition; `level` is the main lever (0..1).
    pub fn set_running(&mut self, level: f64) {
        match self {
            AnyEngine::Turbofan(e) => e.set_running(level),
            AnyEngine::Piston(e) => e.set_running(level),
            AnyEngine::Turboprop(e) => e.set_running(level),
        }
    }

    /// Environment: altitude (m), Mach (the piston engine derives true
    /// airspeed from it), ISA deviation (K), relative humidity (0..1).
    pub fn set_environment(&mut self, altitude_m: f64, mach: f64, delta_isa_k: f64, humidity: f64) {
        match self {
            AnyEngine::Turbofan(e) => {
                e.environment.altitude_m = altitude_m;
                e.environment.mach = mach;
                e.environment.delta_isa_k = delta_isa_k;
            }
            AnyEngine::Piston(e) => {
                let a = crate::atmosphere::isa(altitude_m, delta_isa_k).speed_of_sound_m_s;
                e.environment.altitude_m = altitude_m;
                e.environment.tas_m_s = mach * a;
                e.environment.delta_isa_k = delta_isa_k;
                e.environment.humidity = humidity.clamp(0.0, 1.0);
            }
            AnyEngine::Turboprop(e) => {
                e.environment.altitude_m = altitude_m;
                e.environment.mach = mach;
                e.environment.delta_isa_k = delta_isa_k;
            }
        }
    }

    /// Generic control setter. Names are kind-specific; booleans are 0 / 1.
    pub fn set_control(&mut self, name: &str, value: f64) -> Result<(), String> {
        let on = value >= 0.5;
        match self {
            AnyEngine::Turbofan(e) => {
                let c = &mut e.controls;
                match name {
                    "tla" | "throttle" | "power_lever" => c.tla = value.clamp(0.0, 1.0),
                    "fuel_lever" | "fuel" => c.fuel_lever = on,
                    "starter" => c.starter = on,
                    "anti_ice" => c.anti_ice = on,
                    "pack_bleed" => c.pack_bleed = on,
                    "reverser" => c.reverser = on,
                    "n1_demand" => c.n1_demand_pct = value,
                    _ => return Err(format!("unknown turbofan control {name}")),
                }
            }
            AnyEngine::Piston(e) => {
                let c = &mut e.controls;
                match name {
                    "throttle" | "tla" | "power_lever" => c.throttle = value.clamp(0.0, 1.0),
                    "mixture" => c.mixture = value.clamp(0.0, 1.0),
                    "magnetos" => c.magnetos = value.clamp(0.0, 3.0).round() as u8,
                    "starter" => c.starter = on,
                    "carb_heat" => c.carb_heat = on,
                    "primer" => c.primer_shots = c.primer_shots.saturating_add(value.max(0.0).round() as u32),
                    "fuel_pump" => c.fuel_pump = on,
                    "fuel_selector" => c.fuel_selector = value.clamp(0.0, 3.0).round() as u8,
                    "fuel_left_gal" => c.fuel_left_gal = value.max(0.0),
                    "fuel_right_gal" => c.fuel_right_gal = value.max(0.0),
                    _ => return Err(format!("unknown piston control {name}")),
                }
            }
            AnyEngine::Turboprop(e) => {
                let c = &mut e.controls;
                match name {
                    "power_lever" | "tla" | "throttle" => c.power_lever = value.clamp(-0.3, 1.0),
                    "prop_lever" => c.prop_lever = value.clamp(0.0, 1.0),
                    "condition_lever" => c.condition_lever = value.clamp(0.0, 2.0).round() as u8,
                    "starter" => c.starter = on,
                    "ignition" => c.ignition = on,
                    "bleed_air" => c.bleed_air = on,
                    "inertial_separator" => c.inertial_separator = on,
                    _ => return Err(format!("unknown turboprop control {name}")),
                }
            }
        }
        Ok(())
    }

    fn merge_json<T: Serialize + for<'de> Deserialize<'de> + Copy>(current: T, patch: &str) -> Result<T, String> {
        let mut cur = serde_json::to_value(current).map_err(|e| e.to_string())?;
        let p: serde_json::Value = serde_json::from_str(patch).map_err(|e| e.to_string())?;
        if let (Some(c), Some(p)) = (cur.as_object_mut(), p.as_object()) {
            for (k, v) in p {
                if c.contains_key(k) { c.insert(k.clone(), v.clone()); } else { return Err(format!("unknown fault {k}")); }
            }
        }
        serde_json::from_value(cur).map_err(|e| e.to_string())
    }

    pub fn set_faults_json(&mut self, patch: &str) -> Result<(), String> {
        match self {
            AnyEngine::Turbofan(e) => { e.faults = Self::merge_json(e.faults, patch)?; }
            AnyEngine::Piston(e) => { e.faults = Self::merge_json(e.faults, patch)?; }
            AnyEngine::Turboprop(e) => { e.faults = Self::merge_json(e.faults, patch)?; }
        }
        Ok(())
    }

    pub fn clear_faults(&mut self) {
        match self {
            AnyEngine::Turbofan(e) => e.faults = crate::engine::Faults::none(),
            AnyEngine::Piston(e) => e.faults = crate::piston::PistonFaults::none(),
            AnyEngine::Turboprop(e) => e.faults = crate::turboprop::TpFaults::none(),
        }
    }

    pub fn faults_json(&self) -> String {
        match self {
            AnyEngine::Turbofan(e) => serde_json::to_string(&e.faults).unwrap_or_default(),
            AnyEngine::Piston(e) => serde_json::to_string(&e.faults).unwrap_or_default(),
            AnyEngine::Turboprop(e) => serde_json::to_string(&e.faults).unwrap_or_default(),
        }
    }

    pub fn logger(&mut self) -> &mut DataLogger {
        match self {
            AnyEngine::Turbofan(e) => &mut e.logger,
            AnyEngine::Piston(e) => &mut e.logger,
            AnyEngine::Turboprop(e) => &mut e.logger,
        }
    }

    pub fn logger_ref(&self) -> &DataLogger {
        match self {
            AnyEngine::Turbofan(e) => &e.logger,
            AnyEngine::Piston(e) => &e.logger,
            AnyEngine::Turboprop(e) => &e.logger,
        }
    }
}
