//! Validation of the additional engine presets against their published
//! ratings: IAE V2527-A5 (A320), Lycoming O-320-E2D (Cessna 172),
//! P&WC PT6A-42 (King Air B200). Tolerances are deliberately wide: these
//! presets reuse the calibration of the engine their family was built on.

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{size_design_point, steady_point, CycleInput};
use engine_core::piston::{PistonEngine, PistonSpec};
use engine_core::spec::interp;
use engine_core::turboprop::{tp_size, TurbopropEngine, TurbopropSpec};
use engine_core::{AnySpec, Engine, EngineSpec, Mode, PRESETS};

const PPH: f64 = 7936.64;
const KT: f64 = 0.514444;

fn tf_point(s: &EngineSpec, n1: f64, alt: f64, mach: f64) -> engine_core::cycle::CycleResult {
    let d = size_design_point(s);
    let inl = inlet(isa(alt, 0.0), mach);
    let n1c = n1 / inl.theta.sqrt() / 100.0;
    let n2c = interp(&s.spools.n2_vs_n1, n1c * 100.0) / 100.0;
    steady_point(s, &inl, CycleInput::steady(n1c, n2c, 0.0, None), d.core_nozzle_area_m2)
}

#[test]
fn every_preset_loads_and_reports_its_key() {
    for p in PRESETS {
        let spec = AnySpec::default_for(p.0).unwrap_or_else(|| panic!("preset {} missing", p.0));
        assert_eq!(spec.kind(), p.1, "{}", p.0);
        assert_eq!(spec.preset(), p.0, "{}", p.0);
        let round = AnySpec::from_json(&spec.to_json()).expect("spec JSON round-trips");
        assert_eq!(round.preset(), p.0);
    }
}

// ---------------------------------------------------------------- V2527-A5

#[test]
fn v2500_design_point_and_egt_at_lpt_exit() {
    let s = EngineSpec::v2527_a5();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    assert!((r.net_thrust_n - s.rating.takeoff_thrust_n).abs() / s.rating.takeoff_thrust_n < 0.002);
    assert!((r.bypass_ratio - 4.8).abs() < 0.1, "BPR {:.2}", r.bypass_ratio);
    assert!((26.0..=29.0).contains(&r.overall_pressure_ratio), "OPR {:.1}", r.overall_pressure_ratio);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    assert!((0.33..=0.40).contains(&tsfc), "takeoff TSFC {:.3}", tsfc);
    let egt_c = r.egt_k - 273.15;
    assert!((540.0..=620.0).contains(&egt_c), "takeoff EGT {:.0} C (limit {:.0})", egt_c, s.limits.egt_takeoff_c);
    let idle = tf_point(&s, 20.0, 0.0, 0.0);
    assert!(idle.egt_k - 273.15 < 500.0, "idle EGT {:.0} C", idle.egt_k - 273.15);
    let cruise = tf_point(&s, 85.0, 10_668.0, 0.78);
    let ctsfc = cruise.tsfc * 3600.0 * 9.80665;
    assert!((0.56..=0.68).contains(&ctsfc), "cruise TSFC {:.3}", ctsfc);
}

#[test]
fn v2500_starts_and_accelerates_within_limits() {
    let s = EngineSpec::v2527_a5();
    let mut e = Engine::new(s.clone());
    e.controls.starter = true;
    let (mut t, mut peak, mut fuel_on, mut running) = (0.0, 0.0f64, None, None);
    while t < 150.0 {
        let st = e.state();
        if fuel_on.is_none() && st.n2_pct >= 25.0 { e.controls.fuel_lever = true; fuel_on = Some(t); }
        if st.mode == Mode::Starting { peak = peak.max(st.egt_c); }
        if st.mode == Mode::Running { running = Some(t); break; }
        e.step(0.05); t += 0.05;
    }
    let (f, r) = (fuel_on.expect("motored"), running.expect("idle"));
    assert!(r - f > 20.0 && r - f < 90.0, "start took {:.1} s", r - f);
    assert!(peak > 300.0 && peak < s.limits.egt_start_c, "start EGT peak {:.0} C", peak);
    // Acceleration
    let mut e = Engine::new(s.clone());
    e.set_running(0.0);
    for _ in 0..200 { e.step(0.05); }
    let t0 = e.time();
    e.controls.tla = 1.0;
    let (mut t95, mut peak, mut min_sm) = (None, 0.0f64, 100.0f64);
    while e.time() - t0 < 25.0 {
        e.step(0.02);
        let st = e.state();
        peak = peak.max(st.egt_c); min_sm = min_sm.min(st.hpc_surge_margin_pct);
        if t95.is_none() && st.thrust_n >= 0.95 * s.rating.takeoff_thrust_n { t95 = Some(e.time() - t0); }
    }
    let t95 = t95.expect("95% thrust");
    println!("V2527-A5 accel: 95% thrust in {t95:.1} s, EGT peak {peak:.0} C, min HPC surge margin {min_sm:.1} %");
    // From ground idle (FAR 33.73 counts from flight idle). The EEC's transient
    // EGT limiting trims the accel fuel as the LPT-exit EGT nears the limit,
    // which stretches the last second of the acceleration.
    assert!((3.0..=9.5).contains(&t95), "idle to 95% thrust took {:.1} s", t95);
    let st = e.state();
    assert!(st.n1_pct <= s.limits.n1_max_pct + 0.5 && st.n2_pct <= s.limits.n2_max_pct + 0.5, "N1 {:.1} N2 {:.1}", st.n1_pct, st.n2_pct);
    assert!(peak < s.limits.egt_takeoff_c, "EGT overshoot to {:.0} C (limit {:.0})", peak, s.limits.egt_takeoff_c);
    assert!(min_sm > 2.0 && st.surge_count == 0, "surge margin {:.1}%", min_sm);
    assert!(st.warnings.is_empty(), "warnings: {:?}", st.warnings);
}

#[test]
fn every_turbofan_preset_decelerates_to_idle() {
    // Takeoff to idle: the decel floor must sit below the idle steady fuel or the
    // engine hangs above idle. All turbofan presets should be back within 3 % N1
    // of idle 20 s after the lever is closed.
    for p in PRESETS.iter().filter(|p| p.1 == "turbofan") {
        let AnySpec::Turbofan(spec) = AnySpec::default_for(p.0).unwrap() else { unreachable!() };
        let mut e = Engine::new(spec.clone());
        e.set_running(1.0);
        for _ in 0..200 { e.step(0.05); }
        e.controls.tla = 0.0;
        for _ in 0..400 { e.step(0.05); }
        let st = e.state();
        assert!(st.n1_pct <= spec.spools.n1_idle_pct + 3.0, "{}: N1 {:.1} % still above idle {:.0} % 20 s after closing the lever", p.0, st.n1_pct, spec.spools.n1_idle_pct);
    }
}

// ---------------------------------------------------------------- PW1127G-JM

#[test]
fn pw1100g_geared_fan_design_point_and_cruise() {
    let s = EngineSpec::pw1127g_jm();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    assert!((r.net_thrust_n - s.rating.takeoff_thrust_n).abs() / s.rating.takeoff_thrust_n < 0.002);
    assert!((r.bypass_ratio - 12.5).abs() < 0.15, "BPR {:.2}", r.bypass_ratio);
    assert!((38.0..=42.0).contains(&r.overall_pressure_ratio), "OPR {:.1}", r.overall_pressure_ratio);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    assert!((0.22..=0.30).contains(&tsfc), "takeoff TSFC {:.3}", tsfc);
    let egt_c = r.egt_k - 273.15;
    // Known bias: with the LPT-exit probe the model reads ~790 C at takeoff, below the ~950 C
    // seen in service; the cruise TSFC is ~5 % above P&W's figure. Both move together with the
    // turbine efficiencies, so they cannot be tuned independently in this cycle.
    assert!(egt_c < s.limits.egt_takeoff_c - 50.0 && egt_c > 780.0, "takeoff EGT {:.0} C", egt_c);
    let cruise = tf_point(&s, 85.0, 10_668.0, 0.78);
    let ctsfc = cruise.tsfc * 3600.0 * 9.80665;
    let leap = tf_point(&EngineSpec::leap_1b28(), 85.0, 10_668.0, 0.78);
    let ltsfc = leap.tsfc * 3600.0 * 9.80665;
    assert!((0.46..=0.58).contains(&ctsfc), "cruise TSFC {:.3}", ctsfc);
    assert!(ctsfc < ltsfc + 0.02, "GTF cruise TSFC {:.3} vs LEAP {:.3}", ctsfc, ltsfc);
    // The gearbox: fan at N1, LP turbine 3.06 times faster
    let mut e = Engine::new(s.clone());
    e.set_running(1.0);
    for _ in 0..200 { e.step(0.05); }
    let st = e.state();
    assert!((st.lpt_rpm / st.n1_rpm - 3.0625).abs() < 1e-6, "gear ratio {:.4}", st.lpt_rpm / st.n1_rpm);
    assert!((st.n1_rpm - 3_281.0).abs() < 40.0, "fan rpm at takeoff {:.0}", st.n1_rpm);
}

#[test]
fn pw1100g_starts_and_accelerates_within_limits() {
    let s = EngineSpec::pw1127g_jm();
    let mut e = Engine::new(s.clone());
    e.controls.starter = true;
    let (mut t, mut peak, mut fuel_on, mut running) = (0.0, 0.0f64, None, None);
    while t < 150.0 {
        let st = e.state();
        if fuel_on.is_none() && st.n2_pct >= 25.0 { e.controls.fuel_lever = true; fuel_on = Some(t); }
        if st.mode == Mode::Starting { peak = peak.max(st.egt_c); }
        if st.mode == Mode::Running { running = Some(t); break; }
        e.step(0.05); t += 0.05;
    }
    let (f, r) = (fuel_on.expect("motored"), running.expect("idle"));
    assert!(r - f > 20.0 && r - f < 90.0, "start took {:.1} s", r - f);
    assert!(peak > 300.0 && peak < s.limits.egt_start_c, "start EGT peak {:.0} C", peak);
    let mut e = Engine::new(s.clone());
    e.set_running(0.0);
    for _ in 0..200 { e.step(0.05); }
    let t0 = e.time();
    e.controls.tla = 1.0;
    let (mut t95, mut peak, mut min_sm) = (None, 0.0f64, 100.0f64);
    while e.time() - t0 < 25.0 {
        e.step(0.02);
        let st = e.state();
        peak = peak.max(st.egt_c); min_sm = min_sm.min(st.hpc_surge_margin_pct);
        if t95.is_none() && st.thrust_n >= 0.95 * s.rating.takeoff_thrust_n { t95 = Some(e.time() - t0); }
    }
    let t95 = t95.expect("95% thrust");
    println!("PW1127G-JM accel: 95% thrust in {t95:.1} s, EGT peak {peak:.0} C, min HPC surge margin {min_sm:.1} %");
    // The big geared fan is slow to spin up: the GTF is known for a leisurely spool
    assert!((3.0..=10.0).contains(&t95), "idle to 95% thrust took {:.1} s", t95);
    let st = e.state();
    assert!(peak < s.limits.egt_takeoff_c, "EGT overshoot to {:.0} C", peak);
    assert!(min_sm > 2.0 && st.surge_count == 0, "surge margin {:.1}%", min_sm);
    assert!(st.warnings.is_empty(), "warnings: {:?}", st.warnings);
}

// ---------------------------------------------------------------- O-320-E2D

#[test]
fn o320_static_run_up_and_rated_power() {
    // 172N POH: full-throttle static ~2,280-2,400 rpm; 150 hp at 2,700 rpm
    let mut e = PistonEngine::new(PistonSpec::lycoming_o320());
    e.set_running(1.0);
    for _ in 0..400 { e.step(0.05); }
    let s = e.state();
    assert!((2_200.0..=2_480.0).contains(&s.rpm), "static rpm {:.0}", s.rpm);
    assert!((115.0..=150.0).contains(&s.power_hp), "static power {:.0} hp", s.power_hp);
    assert!((9.5..=14.0).contains(&s.fuel_flow_gph), "fuel flow {:.1} gph", s.fuel_flow_gph);
    let mut e = PistonEngine::new(PistonSpec::lycoming_o320());
    e.environment.tas_m_s = 125.0 * KT;
    e.set_running(1.0);
    for _ in 0..400 { e.step(0.05); }
    let s = e.state();
    assert!((2_550.0..=2_800.0).contains(&s.rpm), "rpm at 125 kt {:.0}", s.rpm);
    assert!((135.0..=165.0).contains(&s.power_hp), "power {:.0} hp at {:.0} rpm", s.power_hp, s.rpm);
}

#[test]
fn o320_cruise_at_8000_ft_leaned() {
    // 172N POH: 8,000 ft, 2,500 rpm ~ 65-70 % power, ~7.5-8.5 gph leaned
    let mut e = PistonEngine::new(PistonSpec::lycoming_o320());
    e.environment.altitude_m = 2_438.0;
    e.environment.tas_m_s = 110.0 * KT;
    e.set_running(1.0);
    e.controls.mixture = 0.6;
    for _ in 0..600 { e.step(0.05); }
    let s = e.state();
    assert!((2_350.0..=2_650.0).contains(&s.rpm), "rpm {:.0}", s.rpm);
    assert!((55.0..=80.0).contains(&s.power_pct), "power {:.0} %", s.power_pct);
    assert!((6.5..=10.0).contains(&s.fuel_flow_gph), "fuel flow {:.1} gph", s.fuel_flow_gph);
}

// ---------------------------------------------------------------- PT6A-42

#[test]
fn pt6a42_design_point_gives_850_shp() {
    let s = TurbopropSpec::pt6a_42();
    let z = tp_size(&s);
    let r = &z.takeoff;
    assert!((r.shaft_power_w / 745.7 - 850.0).abs() < 3.0, "shp {:.0}", r.shaft_power_w / 745.7);
    let itt = r.tt45 - 273.15;
    assert!((650.0..=790.0).contains(&itt), "takeoff ITT {:.0} C (limit 800)", itt);
    let ff = r.wf * PPH;
    assert!((420.0..=560.0).contains(&ff), "takeoff fuel flow {:.0} pph", ff);
}

#[test]
fn pt6a42_takeoff_and_idle_indications() {
    let s = TurbopropSpec::pt6a_42();
    let mut e = TurbopropEngine::new(s.clone());
    e.set_running(1.0);
    for _ in 0..300 { e.step(0.05); }
    let st = e.state();
    assert!((1_950.0..=2_010.0).contains(&st.np_rpm), "Np {:.0}", st.np_rpm);
    // 850 shp at 2,000 rpm = 2,232 ft lb: torque-limited on a standard day
    assert!((1_950.0..=2_240.0).contains(&st.torque_ftlb), "torque {:.0} ft lb", st.torque_ftlb);
    assert!(st.itt_c < s.limits.itt_max_c, "ITT {:.0} C", st.itt_c);
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
    let mut e = TurbopropEngine::new(s.clone());
    e.controls.prop_lever = 1.0;
    e.set_running(0.0);
    for _ in 0..600 { e.step(0.05); }
    let st = e.state();
    assert!((58.0..=64.0).contains(&st.ng_pct), "low idle Ng {:.1}", st.ng_pct);
    assert!(st.itt_c < 600.0, "idle ITT {:.0}", st.itt_c);
    assert!(st.oil_pressure_psi >= s.limits.oil_press_min_idle_psi, "idle oil pressure {:.0}", st.oil_pressure_psi);
}
