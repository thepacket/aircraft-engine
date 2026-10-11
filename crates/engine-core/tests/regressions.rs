//! Regression tests for defects found in review: each one reproduces the
//! original failure case so later tuning cannot silently reintroduce it.

use engine_core::piston::{PistonEngine, PistonSpec};
use engine_core::turbojet::{TjMode, TurbojetEngine, TurbojetSpec};
use engine_core::turboprop::{TpMode, TurbopropEngine, TurbopropSpec};

use engine_core::{Engine, EngineSpec, Mode};

fn run(e: &mut Engine, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as usize {
        e.step(0.05);
    }
}

#[test]
fn start_fuel_and_cycle_fuel_agree_in_the_turboprop_and_turbojet() {
    // Turboprop: motor, fuel on, follow the start; the cycle evaluated at the
    // computed T4 must burn exactly the scheduled fuel.
    let mut e = TurbopropEngine::new(TurbopropSpec::pt6a_114a());
    e.controls.starter = true;
    let mut checked = 0;
    for _ in 0..2_400 {
        let s = e.state();
        if s.ng_pct >= 14.0 { e.controls.condition_lever = 1; }
        if s.mode == TpMode::Starting && s.lit && s.fuel_command_kg_s > 1e-4 {
            assert!((s.cycle.wf - s.fuel_command_kg_s).abs() < 1e-9 * s.fuel_command_kg_s.max(1.0), "PT6 start: scheduled {:.5} vs cycle {:.5} kg/s", s.fuel_command_kg_s, s.cycle.wf);
            checked += 1;
        }
        if s.mode == TpMode::Running { break; }
        e.step(0.05);
    }
    assert!(checked > 50, "turboprop start not exercised ({checked} samples)");
    let mut e = TurbojetEngine::new(TurbojetSpec::j85_ge_5a());
    e.controls.starter = true;
    let mut checked = 0;
    for _ in 0..2_400 {
        let s = e.state();
        if s.n_pct >= 14.0 { e.controls.throttle = 0.0; e.controls.fuel = true; }
        if s.mode == TjMode::Starting && s.lit && s.fuel_command_kg_s > 1e-4 {
            assert!((s.cycle.wf - s.fuel_command_kg_s).abs() < 1e-9 * s.fuel_command_kg_s.max(1.0), "J85 start: scheduled {:.5} vs cycle {:.5} kg/s", s.fuel_command_kg_s, s.cycle.wf);
            checked += 1;
        }
        if s.mode == TjMode::Running { break; }
        e.step(0.05);
    }
    assert!(checked > 50, "turbojet start not exercised ({checked} samples)");
}

#[test]
fn deceleration_at_altitude_does_not_flame_out() {
    // The defect was found with the PW1100G's original 23 % flight idle; the
    // preset has since moved to 26 %, which on its own keeps the fuel above the
    // old pressure-independent threshold, so both idles are run: the original
    // one is what actually exercises the flameout criterion.
    for idle in [23.0, EngineSpec::pw1127g_jm().fadec.idle_n1_flight_pct] {
        for mach in [0.3, 0.4] {
            let mut s = EngineSpec::pw1127g_jm();
            s.fadec.idle_n1_flight_pct = idle;
            let mut e = Engine::new(s);
            e.environment.altitude_m = 10_668.0;
            e.environment.mach = mach;
            e.set_running(0.5);
            run(&mut e, 30.0);
            e.controls.tla = 0.0;
            run(&mut e, 120.0);
            let s = e.state();
            assert_eq!(s.flameout_count, 0, "PW1100G (flight idle {idle} %) flamed out decelerating at FL350 M{mach}");
            assert!(s.lit && s.mode == Mode::Running, "mode {:?} at M{mach}, idle {idle} %", s.mode);
            assert!((s.n1_pct - s.n1_command_pct).abs() < 0.5, "N1 {:.2} vs command {:.2} at M{mach}, idle {idle} %", s.n1_pct, s.n1_command_pct);
        }
    }
}

#[test]
fn hung_start_is_reported_while_the_starter_is_still_engaged() {
    let mut e = Engine::new(EngineSpec::cfm56_7b26());
    e.faults.start_fuel_factor = 0.1; // starved start schedule
    e.controls.starter = true;
    let mut hung = false;
    for _ in 0..(300.0 / 0.05) as usize {
        let s = e.state();
        if s.n2_pct >= 25.0 { e.controls.fuel_lever = true; }
        if s.warnings.iter().any(|w| w.starts_with("HUNG START")) { assert!(s.starter_engaged, "warning only after the starter dropped out"); hung = true; break; }
        e.step(0.05);
    }
    assert!(hung, "no HUNG START warning in 5 min: mode {:?} N2 {:.1}", e.state().mode, e.state().n2_pct);
    // A normal start stays warning-free
    let mut e = Engine::new(EngineSpec::cfm56_7b26());
    e.controls.starter = true;
    for _ in 0..(150.0 / 0.05) as usize {
        let s = e.state();
        if s.n2_pct >= 25.0 { e.controls.fuel_lever = true; }
        assert!(!s.warnings.iter().any(|w| w.starts_with("HUNG START")), "false hung start at N2 {:.1}", s.n2_pct);
        if s.mode == Mode::Running { break; }
        e.step(0.05);
    }
    assert_eq!(e.state().mode, Mode::Running);
}

#[test]
fn leaning_from_full_rich_raises_power_and_rpm_at_altitude() {
    let mut rich = PistonEngine::new(PistonSpec::lycoming_o360());
    rich.environment.altitude_m = 2_438.0;
    rich.environment.tas_m_s = 55.0;
    rich.set_running(0.8);
    rich.controls.mixture = 1.0;
    for _ in 0..1_200 { rich.step(0.05); }
    let mut lean = PistonEngine::new(PistonSpec::lycoming_o360());
    lean.environment.altitude_m = 2_438.0;
    lean.environment.tas_m_s = 55.0;
    lean.set_running(0.8);
    lean.controls.mixture = 0.6;
    for _ in 0..1_200 { lean.step(0.05); }
    let (r, l) = (rich.state(), lean.state());
    assert!(l.power_hp > r.power_hp + 2.0, "leaning should add power: {:.1} -> {:.1} hp", r.power_hp, l.power_hp);
    assert!(l.rpm > r.rpm + 10.0, "leaning should raise rpm: {:.0} -> {:.0}", r.rpm, l.rpm);
    assert!(l.fuel_flow_gph < r.fuel_flow_gph - 1.0, "and cut fuel: {:.1} -> {:.1} gph", r.fuel_flow_gph, l.fuel_flow_gph);
}

#[test]
fn stopped_piston_engine_cools_to_ambient() {
    let mut e = PistonEngine::new(PistonSpec::lycoming_o360());
    for _ in 0..(1_800.0 / 0.05) as usize { e.step(0.05); }
    let s = e.state();
    assert!((s.cht_f - 59.0).abs() < 1.0, "CHT {:.1} F after 30 min stopped at 59 F", s.cht_f);
    assert!((s.oil_temperature_f - 59.0).abs() < 1.0, "oil {:.1} F after 30 min stopped at 59 F", s.oil_temperature_f);
}

#[test]
fn carburettor_ice_forms_in_warm_humid_air_at_part_throttle() {
    // AC 20-113: serious icing to ~27 C at cruise power, ~38 C at glide power
    for (oat_c, throttle) in [(20.0, 0.5), (25.0, 0.5), (28.0, 0.5), (30.0, 0.15), (35.0, 0.15)] {
        let mut e = PistonEngine::new(PistonSpec::lycoming_o360());
        e.environment.delta_isa_k = oat_c - 15.0;
        e.environment.humidity = 0.9;
        e.set_running(throttle);
        for _ in 0..4_800 { e.step(0.05); }
        assert!(e.state().carb_ice > 0.3, "ice {:.2} at {oat_c} C, 90 % humidity, throttle {throttle}", e.state().carb_ice);
    }
    // and not in dry air
    let mut e = PistonEngine::new(PistonSpec::lycoming_o360());
    e.environment.delta_isa_k = 5.0;
    e.environment.humidity = 0.3;
    e.set_running(0.5);
    for _ in 0..4_800 { e.step(0.05); }
    assert_eq!(e.state().carb_ice, 0.0);
}

#[test]
fn pt6a42_ground_start_reaches_idle_without_oil_warnings() {
    let mut e = TurbopropEngine::new(TurbopropSpec::pt6a_42());
    e.controls.prop_lever = 1.0;
    e.controls.starter = true;
    let mut fuel = false;
    let mut t = 0.0;
    while t < 90.0 {
        let s = e.state();
        if !fuel && s.ng_pct >= 13.0 { e.controls.condition_lever = 1; fuel = true; }
        if e.controls.starter && s.ng_pct >= 52.0 { e.controls.starter = false; }
        assert!(!s.warnings.iter().any(|w| w.starts_with("OIL STARVATION") || w.starts_with("HUNG START")), "t={t:.0}: {:?}", s.warnings);
        if s.mode == TpMode::Running { break; }
        e.step(0.05);
        t += 0.05;
    }
    let s = e.state();
    assert_eq!(s.mode, TpMode::Running, "start hung at Ng {:.1} %", s.ng_pct);
    assert!(t < 60.0, "start took {t:.0} s");
    e.controls.condition_lever = 2;
    for _ in 0..600 { e.step(0.05); }
    assert!(e.state().ng_pct >= 68.0, "high idle Ng {:.1}", e.state().ng_pct);
}

#[test]
fn turboprop_oil_pressure_is_normal_at_cruise_and_takeoff_power() {
    for (spec, lever) in [(TurbopropSpec::pt6a_114a(), 0.7), (TurbopropSpec::pt6a_114a(), 1.0), (TurbopropSpec::pt6a_42(), 0.7), (TurbopropSpec::pt6a_42(), 1.0)] {
        let mut e = TurbopropEngine::new(spec.clone());
        e.environment.altitude_m = 2_500.0;
        e.environment.mach = 0.25;
        e.controls.prop_lever = 1.0;
        e.set_running(lever);
        for _ in 0..600 { e.step(0.05); }
        let s = e.state();
        assert!(!s.warnings.iter().any(|w| w.starts_with("LOW OIL")), "{} at lever {lever}: {:?} (oil {:.0} psi, Ng {:.1})", spec.name, s.warnings, s.oil_pressure_psi, s.ng_pct);
        assert!(s.oil_pressure_psi <= spec.oil.press_relief_psi + 0.5, "{}: {:.0} psi above the relief setting", spec.name, s.oil_pressure_psi);
    }
}
