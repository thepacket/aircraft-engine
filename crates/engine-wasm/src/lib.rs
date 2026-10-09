//! WebAssembly bindings. The whole simulation runs inside the browser; the
//! web server only delivers this module and the static UI.

use engine_core::{Engine, EngineSpec, Faults};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Simulator {
    engine: Engine,
}

#[wasm_bindgen]
impl Simulator {
    /// Create a simulator from a spec JSON string, or the default CFM56-7B26
    /// when `spec_json` is empty.
    #[wasm_bindgen(constructor)]
    pub fn new(spec_json: &str) -> Result<Simulator, JsValue> {
        let spec = if spec_json.trim().is_empty() {
            EngineSpec::cfm56_7b26()
        } else {
            EngineSpec::from_json(spec_json).map_err(|e| JsValue::from_str(&e))?
        };
        Ok(Simulator { engine: Engine::new(spec) })
    }

    pub fn default_spec_json() -> String {
        EngineSpec::cfm56_7b26().to_json()
    }

    pub fn version() -> String {
        engine_core::VERSION.to_string()
    }

    /// Replace the engine spec and reset to cold and dark.
    pub fn set_spec_json(&mut self, spec_json: &str) -> Result<(), JsValue> {
        let spec = EngineSpec::from_json(spec_json).map_err(|e| JsValue::from_str(&e))?;
        self.engine.set_spec(spec);
        Ok(())
    }

    pub fn spec_json(&self) -> String {
        self.engine.spec.to_json()
    }

    pub fn sizing_json(&self) -> String {
        serde_json::to_string(&self.engine.sizing).unwrap_or_default()
    }

    pub fn reset(&mut self) {
        self.engine.reset();
    }

    pub fn set_throttle(&mut self, tla: f64) {
        self.engine.controls.tla = tla.clamp(0.0, 1.0);
    }

    pub fn set_fuel_lever(&mut self, on: bool) {
        self.engine.controls.fuel_lever = on;
    }

    pub fn set_starter(&mut self, on: bool) {
        self.engine.controls.starter = on;
    }

    pub fn set_anti_ice(&mut self, on: bool) {
        self.engine.controls.anti_ice = on;
    }

    pub fn set_pack_bleed(&mut self, on: bool) {
        self.engine.controls.pack_bleed = on;
    }

    pub fn set_reverser(&mut self, on: bool) {
        self.engine.controls.reverser = on;
    }

    /// Direct N1 demand in % (autothrottle / log replay); negative clears it.
    pub fn set_n1_demand(&mut self, pct: f64) {
        self.engine.controls.n1_demand_pct = pct;
    }

    /// Replace the fault set from JSON (all fields optional, missing = unchanged).
    pub fn set_faults_json(&mut self, json: &str) -> Result<(), JsValue> {
        let mut current = serde_json::to_value(self.engine.faults).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let patch: serde_json::Value = serde_json::from_str(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
        if let (Some(cur), Some(p)) = (current.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                cur.insert(k.clone(), v.clone());
            }
        }
        self.engine.faults = serde_json::from_value::<Faults>(current).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(())
    }

    pub fn clear_faults(&mut self) {
        self.engine.faults = Faults::none();
    }

    pub fn faults_json(&self) -> String {
        serde_json::to_string(&self.engine.faults).unwrap_or_default()
    }

    pub fn set_environment(&mut self, altitude_m: f64, mach: f64, delta_isa_k: f64) {
        self.engine.environment.altitude_m = altitude_m;
        self.engine.environment.mach = mach;
        self.engine.environment.delta_isa_k = delta_isa_k;
    }

    /// Jump to a stabilised running condition at the given throttle.
    pub fn set_running(&mut self, tla: f64) {
        self.engine.set_running(tla);
    }

    pub fn step(&mut self, dt: f64) {
        self.engine.step(dt);
    }

    pub fn time(&self) -> f64 {
        self.engine.time()
    }

    /// Full engine state as JSON.
    pub fn state_json(&self) -> String {
        serde_json::to_string(&self.engine.state()).unwrap_or_default()
    }

    // ---- Data logger ----
    pub fn logger_start(&mut self) {
        let t = self.engine.time();
        self.engine.logger.start(t);
    }

    pub fn logger_stop(&mut self) {
        self.engine.logger.stop();
    }

    pub fn logger_clear(&mut self) {
        self.engine.logger.clear();
    }

    pub fn logger_set_rate(&mut self, hz: f64) {
        self.engine.logger.set_rate(hz);
    }

    pub fn logger_rows(&self) -> usize {
        self.engine.logger.len()
    }

    pub fn logger_csv(&self) -> String {
        self.engine.logger.to_csv()
    }
}
