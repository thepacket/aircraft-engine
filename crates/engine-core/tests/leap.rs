//! Validation of the CFM LEAP-1B28 preset (Boeing 737 MAX) against its
//! published rating and limits, and against the CFM56-7B26 it replaces.
//!
//! Sources: FAA TCDS E00088EN (rating, speeds), Boeing 737 MAX FCOM
//! limitations (EGT, N1 / N2, oil), CFM published data (fan diameter, bypass
//! ratio, pressure ratio class, ~15 % lower fuel burn than the CFM56-7B).

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{size_design_point, steady_point, CycleInput};
use engine_core::spec::interp;
use engine_core::{Engine, EngineSpec, Mode};

const LBF: f64 = 4.448222;
const PPH: f64 = 7936.64;

fn spec() -> EngineSpec {
    EngineSpec::leap_1b28()
}

fn point(s: &EngineSpec, n1: f64, alt: f64, mach: f64) -> engine_core::cycle::CycleResult {
    let d = size_design_point(s);
    let inl = inlet(isa(alt, 0.0), mach);
    let n1c = n1 / inl.theta.sqrt() / 100.0;
    let n2c = interp(&s.spools.n2_vs_n1, n1c * 100.0) / 100.0;
    steady_point(s, &inl, CycleInput::steady(n1c, n2c, 0.0, None), d.core_nozzle_area_m2)
}

fn run_for(e: &mut Engine, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as usize {
        e.step(0.05);
    }
}

#[test]
fn design_point_reproduces_rated_thrust_and_cycle_class() {
    let s = spec();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    assert!((r.net_thrust_n - s.rating.takeoff_thrust_n).abs() / s.rating.takeoff_thrust_n < 0.002, "thrust {:.0} N", r.net_thrust_n);
    assert!((r.bypass_ratio - 8.6).abs() < 0.1, "BPR {:.2}", r.bypass_ratio);
    assert!((39.0..=42.0).contains(&r.overall_pressure_ratio), "OPR {:.1}", r.overall_pressure_ratio);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    assert!((0.26..=0.32).contains(&tsfc), "takeoff TSFC {:.3} lb/lbf/h", tsfc);
    let egt_c = r.egt_k - 273.15;
    assert!(egt_c < s.limits.egt_takeoff_c - 50.0 && egt_c > 820.0, "takeoff EGT {:.0} C vs limit {:.0}", egt_c, s.limits.egt_takeoff_c);
    assert!((1_750.0..=1_950.0).contains(&d.t4_takeoff_k), "T4 {:.0} K", d.t4_takeoff_k);
}

#[test]
fn burns_less_fuel_than_the_cfm56_at_cruise() {
    let leap = point(&spec(), 85.0, 10_668.0, 0.78);
    let cfm = point(&EngineSpec::cfm56_7b26(), 85.0, 10_668.0, 0.78);
    let tsfc_leap = leap.tsfc * 3600.0 * 9.80665;
    let tsfc_cfm = cfm.tsfc * 3600.0 * 9.80665;
    assert!((0.50..=0.58).contains(&tsfc_leap), "LEAP cruise TSFC {:.3}", tsfc_leap);
    let gain = 1.0 - tsfc_leap / tsfc_cfm;
    assert!((0.08..=0.20).contains(&gain), "LEAP cruise TSFC gain over CFM56 {:.1} %", gain * 100.0);
    let thrust_lbf = leap.net_thrust_n / LBF;
    assert!((3_000.0..=6_000.0).contains(&thrust_lbf), "cruise thrust {:.0} lbf", thrust_lbf);
}

#[test]
fn ground_idle_is_realistic() {
    let r = point(&spec(), 19.0, 0.0, 0.0);
    let wf_pph = r.wf_kg_s * PPH;
    assert!((700.0..=1_300.0).contains(&wf_pph), "idle fuel flow {:.0} lb/h", wf_pph);
    assert!(r.egt_k - 273.15 < 650.0, "idle EGT {:.0} C", r.egt_k - 273.15);
}

#[test]
fn ground_start_reaches_idle_within_the_start_limit() {
    let mut e = Engine::new(spec());
    e.controls.starter = true;
    let (mut t, mut peak_egt, mut fuel_on_at, mut running_at) = (0.0, 0.0f64, None, None);
    while t < 150.0 {
        let st = e.state();
        if fuel_on_at.is_none() && st.n2_pct >= 25.0 { e.controls.fuel_lever = true; fuel_on_at = Some(t); }
        if st.mode == Mode::Starting { peak_egt = peak_egt.max(st.egt_c); }
        if st.mode == Mode::Running { running_at = Some(t); break; }
        e.step(0.05);
        t += 0.05;
    }
    let fuel_on = fuel_on_at.expect("motored to 25% N2");
    let running = running_at.expect("reached idle");
    assert!(running - fuel_on > 20.0 && running - fuel_on < 90.0, "start took {:.1} s", running - fuel_on);
    assert!(peak_egt > 350.0 && peak_egt < spec().limits.egt_start_c, "start EGT peak {:.0} C", peak_egt);
    e.controls.starter = false;
    run_for(&mut e, 30.0);
    let st = e.state();
    assert!((st.n1_pct - 19.0).abs() < 1.5, "idle N1 {:.1}", st.n1_pct);
    assert!((st.n2_pct - 58.0).abs() < 1.5, "idle N2 {:.1}", st.n2_pct);
    assert!(st.warnings.is_empty(), "warnings at idle: {:?}", st.warnings);
}

#[test]
fn ground_idle_to_takeoff_acceleration_time() {
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    run_for(&mut e, 10.0);
    let t0 = e.time();
    e.controls.tla = 1.0;
    let (mut t95, mut peak_egt, mut min_sm) = (None, 0.0f64, 100.0f64);
    while e.time() - t0 < 25.0 {
        e.step(0.02);
        let st = e.state();
        peak_egt = peak_egt.max(st.egt_c);
        min_sm = min_sm.min(st.hpc_surge_margin_pct);
        if t95.is_none() && st.thrust_n >= 0.95 * spec().rating.takeoff_thrust_n { t95 = Some(e.time() - t0); }
    }
    let t95 = t95.expect("reached 95% thrust");
    assert!((3.0..=8.0).contains(&t95), "idle to 95% thrust took {:.1} s", t95);
    let st = e.state();
    assert!((st.n1_pct - 100.0).abs() < 1.0, "takeoff N1 {:.1}", st.n1_pct);
    assert!(st.n2_pct < spec().limits.n2_max_pct, "takeoff N2 {:.1} over the limit", st.n2_pct);
    assert!(peak_egt < spec().limits.egt_takeoff_c, "EGT overshoot to {:.0} C", peak_egt);
    assert!(min_sm > 2.0 && st.surge_count == 0, "surge margin {:.1}%", min_sm);
    assert!(st.warnings.is_empty(), "warnings during takeoff: {:?}", st.warnings);
}

#[test]
fn steady_state_helper_matches_the_running_engine() {
    use engine_core::{AnyEngine, AnySpec};
    let e = AnyEngine::new(AnySpec::default_for("leap1b").unwrap());
    let json = e.steady_state_json(1.0, -1.0, 0.0, 0.0, 0.0, 0.4, 30.0);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let thrust = v["thrust_n"].as_f64().unwrap();
    assert!((thrust - spec().rating.takeoff_thrust_n).abs() / spec().rating.takeoff_thrust_n < 0.02, "steady takeoff thrust {:.0} N", thrust);
    assert_eq!(e.time(), 0.0, "the original engine must be untouched");
    // Holding N1 instead of the lever
    let held: serde_json::Value = serde_json::from_str(&e.steady_state_json(0.0, 85.0, 0.0, 0.0, 0.0, 0.4, 30.0)).unwrap();
    let n1 = held["n1_pct"].as_f64().unwrap();
    assert!((n1 - 85.0).abs() < 0.5, "held N1 {:.1}", n1);
}
