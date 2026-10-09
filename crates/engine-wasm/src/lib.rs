//! WebAssembly bindings. The whole simulation runs inside the browser; the
//! web server only delivers this module and the static UI.

use engine_core::{AnyEngine, AnySpec};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Simulator {
    engine: AnyEngine,
}

fn js(e: String) -> JsValue {
    JsValue::from_str(&e)
}

#[wasm_bindgen]
impl Simulator {
    /// Create a simulator from a spec JSON string (any engine kind), or the
    /// default CFM56-7B26 turbofan when `spec_json` is empty.
    #[wasm_bindgen(constructor)]
    pub fn new(spec_json: &str) -> Result<Simulator, JsValue> {
        let spec = if spec_json.trim().is_empty() { AnySpec::default_for("turbofan").unwrap() } else { AnySpec::from_json(spec_json).map_err(js)? };
        Ok(Simulator { engine: AnyEngine::new(spec) })
    }

    /// Default spec JSON for an engine kind: "turbofan", "piston", "turboprop".
    pub fn default_spec_json_for(kind: &str) -> Result<String, JsValue> {
        AnySpec::default_for(kind).map(|s| s.to_json()).ok_or_else(|| js(format!("unknown engine kind {kind}")))
    }

    pub fn default_spec_json() -> String {
        AnySpec::default_for("turbofan").unwrap().to_json()
    }

    pub fn version() -> String {
        engine_core::VERSION.to_string()
    }

    pub fn kind(&self) -> String {
        self.engine.kind().to_string()
    }

    /// Replace the engine spec (may change the engine kind) and reset.
    pub fn set_spec_json(&mut self, spec_json: &str) -> Result<(), JsValue> {
        let spec = AnySpec::from_json(spec_json).map_err(js)?;
        let rate = self.engine.logger_ref().rate_hz;
        self.engine = AnyEngine::new(spec);
        self.engine.logger().set_rate(rate);
        Ok(())
    }

    pub fn spec_json(&self) -> String {
        self.engine.spec().to_json()
    }

    pub fn sizing_json(&self) -> String {
        self.engine.sizing_json()
    }

    pub fn reset(&mut self) {
        self.engine.reset();
    }

    /// Generic control: name is kind-specific (see the engine documentation).
    pub fn set_control(&mut self, name: &str, value: f64) -> Result<(), JsValue> {
        self.engine.set_control(name, value).map_err(js)
    }

    // Legacy turbofan-style setters, mapped onto the generic control names.
    pub fn set_throttle(&mut self, v: f64) { let _ = self.engine.set_control("tla", v); }
    pub fn set_fuel_lever(&mut self, on: bool) { let _ = self.engine.set_control("fuel_lever", on as u8 as f64); }
    pub fn set_starter(&mut self, on: bool) { let _ = self.engine.set_control("starter", on as u8 as f64); }
    pub fn set_anti_ice(&mut self, on: bool) { let _ = self.engine.set_control("anti_ice", on as u8 as f64); }
    pub fn set_pack_bleed(&mut self, on: bool) { let _ = self.engine.set_control("pack_bleed", on as u8 as f64); }
    pub fn set_reverser(&mut self, on: bool) { let _ = self.engine.set_control("reverser", on as u8 as f64); }
    pub fn set_n1_demand(&mut self, pct: f64) { let _ = self.engine.set_control("n1_demand", pct); }

    /// Environment: altitude (m), Mach, ISA deviation (K), relative humidity (0..1).
    pub fn set_environment(&mut self, altitude_m: f64, mach: f64, delta_isa_k: f64) {
        self.engine.set_environment(altitude_m, mach, delta_isa_k, 0.4);
    }

    pub fn set_environment_full(&mut self, altitude_m: f64, mach: f64, delta_isa_k: f64, humidity: f64) {
        self.engine.set_environment(altitude_m, mach, delta_isa_k, humidity);
    }

    pub fn set_running(&mut self, level: f64) {
        self.engine.set_running(level);
    }

    pub fn step(&mut self, dt: f64) {
        self.engine.step(dt);
    }

    pub fn time(&self) -> f64 {
        self.engine.time()
    }

    pub fn state_json(&self) -> String {
        self.engine.state_json()
    }

    pub fn set_faults_json(&mut self, json: &str) -> Result<(), JsValue> {
        self.engine.set_faults_json(json).map_err(js)
    }

    pub fn clear_faults(&mut self) {
        self.engine.clear_faults();
    }

    pub fn faults_json(&self) -> String {
        self.engine.faults_json()
    }

    // ---- Data logger ----
    pub fn logger_start(&mut self) {
        let t = self.engine.time();
        self.engine.logger().start(t);
    }
    pub fn logger_stop(&mut self) { self.engine.logger().stop(); }
    pub fn logger_clear(&mut self) { self.engine.logger().clear(); }
    pub fn logger_set_rate(&mut self, hz: f64) { self.engine.logger().set_rate(hz); }
    pub fn logger_rows(&self) -> usize { self.engine.logger_ref().len() }
    pub fn logger_csv(&self) -> String { self.engine.logger_ref().to_csv() }
}
