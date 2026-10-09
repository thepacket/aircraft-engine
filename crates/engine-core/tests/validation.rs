//! Validation of the CFM56-7B26 model against published operating points.
//!
//! Sources: FAA TCDS E00055EN, Boeing 737NG FCOM (limitations, engine
//! indications), CFM56-7B published performance, FAR 33.73 (accel time).
//! Tolerances are deliberately explicit so a student can see what the model
//! is and is not expected to reproduce.

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{cycle, size_design_point, solve_t4_for_nozzle, CycleInput};
use engine_core::spec::interp;
use engine_core::{Engine, EngineSpec, Mode};

const LBF: f64 = 4.448222;
const PPH: f64 = 7936.64; // kg/s -> lb/h

fn spec() -> EngineSpec {
    EngineSpec::cfm56_7b26()
}

#[test]
fn design_point_reproduces_rated_thrust() {
    let s = spec();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    let err = (r.net_thrust_n - s.rating.takeoff_thrust_n).abs() / s.rating.takeoff_thrust_n;
    assert!(err < 0.002, "thrust {:.0} N vs rated {:.0} N", r.net_thrust_n, s.rating.takeoff_thrust_n);
    assert!(d.core_nozzle_area_m2 > 0.1 && d.core_nozzle_area_m2 < 1.0, "A9 = {:.3} m2", d.core_nozzle_area_m2);
    assert!(d.bypass_nozzle_area_m2 > 0.5 && d.bypass_nozzle_area_m2 < 2.5, "A19 = {:.3} m2", d.bypass_nozzle_area_m2);
    // Published OPR 32.7
    assert!((r.overall_pressure_ratio - 32.7).abs() < 0.2);
}

#[test]
fn takeoff_point_fuel_flow_and_egt_are_realistic() {
    let s = spec();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    let wf_pph = r.wf_kg_s * PPH;
    // CFM56-7B26 takeoff fuel flow ~ 9,000-10,000 lb/h (TSFC ~0.35-0.38)
    assert!((8_500.0..=10_500.0).contains(&wf_pph), "takeoff fuel flow {:.0} lb/h", wf_pph);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    assert!((0.32..=0.40).contains(&tsfc), "takeoff TSFC {:.3} lb/lbf/h", tsfc);
    // Takeoff EGT on a new engine, ISA SL: roughly 800-900 C (redline 950)
    let egt_c = r.egt_k - 273.15;
    assert!((780.0..=920.0).contains(&egt_c), "takeoff EGT {:.0} C", egt_c);
    // Turbine inlet temperature in the expected class
    assert!((1_500.0..=1_800.0).contains(&d.t4_takeoff_k), "T4 {:.0} K", d.t4_takeoff_k);
    // HPC delivery temperature ~ 550-650 C
    let t3 = r.stations.iter().find(|x| x.id == "3").unwrap().tt_k - 273.15;
    assert!((520.0..=680.0).contains(&t3), "T3 {:.0} C", t3);
}

#[test]
fn ground_idle_matches_cockpit_indications() {
    let s = spec();
    let d = size_design_point(&s);
    let inl = inlet(isa(0.0, 0.0), 0.0);
    let n1c = s.spools.n1_idle_pct / 100.0;
    let n2c = interp(&s.spools.n2_vs_n1, s.spools.n1_idle_pct) / 100.0;
    let t4 = solve_t4_for_nozzle(&s, &inl, n1c, n2c, d.core_nozzle_area_m2);
    let r = cycle(&s, &inl, CycleInput { n1c, n2c, t4_k: t4, a9_m2: Some(d.core_nozzle_area_m2), combustion: true });
    let wf_pph = r.wf_kg_s * PPH;
    let egt_c = r.egt_k - 273.15;
    let thrust_lbf = r.net_thrust_n / LBF;
    // 737NG ground idle: FF ~ 900-1,300 lb/h, EGT ~ 380-480 C, thrust ~ 1,000-2,000 lbf
    assert!((800.0..=1_400.0).contains(&wf_pph), "idle fuel flow {:.0} lb/h", wf_pph);
    assert!((350.0..=500.0).contains(&egt_c), "idle EGT {:.0} C", egt_c);
    assert!((600.0..=2_500.0).contains(&thrust_lbf), "idle thrust {:.0} lbf", thrust_lbf);
}

#[test]
fn cruise_point_is_realistic() {
    // FL350, M0.78, N1 ~ 85%: thrust ~ 4,500-6,000 lbf per engine, TSFC ~ 0.60-0.65
    let s = spec();
    let d = size_design_point(&s);
    let inl = inlet(isa(10_668.0, 0.0), 0.78);
    let n1 = 85.0;
    let n1c = n1 / inl.theta.sqrt() / 100.0;
    let n2c = interp(&s.spools.n2_vs_n1, n1c * 100.0) / 100.0;
    let t4 = solve_t4_for_nozzle(&s, &inl, n1c, n2c, d.core_nozzle_area_m2);
    let r = cycle(&s, &inl, CycleInput { n1c, n2c, t4_k: t4, a9_m2: Some(d.core_nozzle_area_m2), combustion: true });
    let thrust_lbf = r.net_thrust_n / LBF;
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    let wf_pph = r.wf_kg_s * PPH;
    assert!((3_500.0..=7_500.0).contains(&thrust_lbf), "cruise thrust {:.0} lbf", thrust_lbf);
    assert!((0.52..=0.72).contains(&tsfc), "cruise TSFC {:.3}", tsfc);
    assert!((2_000.0..=4_000.0).contains(&wf_pph), "cruise fuel flow {:.0} lb/h", wf_pph);
    assert!(r.propulsive_efficiency > 0.5, "propulsive efficiency {:.2}", r.propulsive_efficiency);
}

#[test]
fn thrust_and_fuel_increase_monotonically_with_n1() {
    let s = spec();
    let d = size_design_point(&s);
    for (alt, mach) in [(0.0, 0.0), (0.0, 0.25), (3_000.0, 0.5), (10_668.0, 0.78)] {
        let inl = inlet(isa(alt, 0.0), mach);
        let mut last_thrust = -1.0;
        let mut last_wf = -1.0;
        let mut n1 = 21.0;
        while n1 <= 104.0 {
            let n1c = n1 / inl.theta.sqrt() / 100.0;
            let n2c = interp(&s.spools.n2_vs_n1, n1c * 100.0) / 100.0;
            let t4 = solve_t4_for_nozzle(&s, &inl, n1c, n2c, d.core_nozzle_area_m2);
            let r = cycle(&s, &inl, CycleInput { n1c, n2c, t4_k: t4, a9_m2: Some(d.core_nozzle_area_m2), combustion: true });
            assert!(t4 > 400.0 && t4 < 2_500.0, "T4 {:.0} K at N1 {} alt {} M {}", t4, n1, alt, mach);
            assert!(r.egt_k > 400.0 && r.egt_k < 1_400.0, "EGT {:.0} K at N1 {} alt {} M {}", r.egt_k, n1, alt, mach);
            assert!(r.net_thrust_n >= last_thrust - 1.0, "thrust not monotonic at N1 {} alt {} M {}", n1, alt, mach);
            assert!(r.wf_kg_s >= last_wf - 1e-6, "fuel not monotonic at N1 {} alt {} M {}", n1, alt, mach);
            last_thrust = r.net_thrust_n;
            last_wf = r.wf_kg_s;
            n1 += 1.0;
        }
    }
}

#[test]
fn ground_start_sequence_reaches_idle_within_limits() {
    let mut e = Engine::new(spec());
    let dt = 0.05;
    e.controls.starter = true;
    let mut t = 0.0;
    let mut peak_egt: f64 = 0.0;
    let mut fuel_on_at = None;
    let mut running_at = None;
    while t < 150.0 {
        let st = e.state();
        if fuel_on_at.is_none() && st.n2_pct >= 25.0 {
            e.controls.fuel_lever = true;
            fuel_on_at = Some(t);
        }
        if st.mode == Mode::Starting {
            peak_egt = peak_egt.max(st.egt_c);
        }
        if st.mode == Mode::Running && running_at.is_none() {
            running_at = Some(t);
            e.controls.starter = false;
            break;
        }
        e.step(dt);
        t += dt;
    }
    let fuel_on = fuel_on_at.expect("starter motored to 25% N2");
    assert!(fuel_on < 30.0, "took {:.1} s to reach 25% N2", fuel_on);
    let running = running_at.expect("engine reached idle");
    // Typical CFM56-7B ground start: ~40-60 s from fuel on to stabilised idle
    assert!(running - fuel_on > 20.0 && running - fuel_on < 90.0, "start took {:.1} s", running - fuel_on);
    assert!(peak_egt < spec().limits.egt_start_c, "start EGT peaked at {:.0} C", peak_egt);
    assert!(peak_egt > 350.0, "start EGT peak only {:.0} C (no light-off?)", peak_egt);

    // Let it stabilise
    for _ in 0..400 {
        e.step(dt);
    }
    let st = e.state();
    assert!((st.n1_pct - 21.0).abs() < 1.5, "idle N1 {:.1}", st.n1_pct);
    assert!((st.n2_pct - 60.0).abs() < 1.5, "idle N2 {:.1}", st.n2_pct);
    assert!(st.oil_pressure_psi > spec().limits.oil_pressure_min_psi);
}

#[test]
fn idle_to_takeoff_acceleration_meets_far_33_73() {
    // FAR 33.73: from flight idle to 95% rated takeoff thrust in not more than 5 s.
    // Ground idle (lower) to 95% N1 in a CFM56-7B takes ~5-7 s; allow 8.
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    for _ in 0..200 {
        e.step(0.05);
    }
    let t0 = e.time();
    e.controls.tla = 1.0;
    let mut t95 = None;
    let mut peak_egt: f64 = 0.0;
    let mut max_ff: f64 = 0.0;
    while e.time() - t0 < 20.0 {
        e.step(0.02);
        let st = e.state();
        peak_egt = peak_egt.max(st.egt_c);
        max_ff = max_ff.max(st.fuel_flow_kg_s);
        if t95.is_none() && st.thrust_n >= 0.95 * spec().rating.takeoff_thrust_n {
            t95 = Some(e.time() - t0);
        }
    }
    let t95 = t95.expect("reached 95% thrust");
    assert!(t95 <= 8.0, "idle to 95% thrust took {:.1} s", t95);
    assert!(t95 >= 3.0, "idle to 95% thrust took only {:.1} s (unrealistically fast)", t95);
    let st = e.state();
    assert!((st.n1_pct - 100.0).abs() < 1.0, "takeoff N1 {:.1}", st.n1_pct);
    assert!(peak_egt < spec().limits.egt_takeoff_c, "EGT overshoot to {:.0} C", peak_egt);
    assert!(st.warnings.is_empty(), "warnings during takeoff: {:?}", st.warnings);
}

#[test]
fn shutdown_spools_down_and_cools() {
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    for _ in 0..100 {
        e.step(0.05);
    }
    e.controls.fuel_lever = false;
    let mut t = 0.0;
    while t < 120.0 {
        e.step(0.05);
        t += 0.05;
    }
    let st = e.state();
    assert!(st.n2_pct < 2.0, "N2 {:.1} after 120 s", st.n2_pct);
    assert!(st.n1_pct < 5.0, "N1 {:.1} after 120 s", st.n1_pct);
    assert!(st.fuel_flow_kg_s < 1e-3);
    assert!(st.egt_c < 200.0, "EGT {:.0} C after 120 s", st.egt_c);
    assert_eq!(st.mode, Mode::Off);
}

#[test]
fn logger_records_csv_at_requested_rate() {
    let mut e = Engine::new(spec());
    e.set_running(0.5);
    e.logger.set_rate(20.0);
    e.logger.start(e.time());
    for _ in 0..100 {
        e.step(0.05);
    }
    e.logger.stop();
    let n = e.logger.len();
    assert!((99..=101).contains(&n), "{} rows for 5 s at 20 Hz", n);
    let csv = e.logger.to_csv();
    let header = csv.lines().next().unwrap();
    assert!(header.starts_with("time_s,mode,tla"));
    assert_eq!(csv.lines().count(), n + 1);
    let cols = header.split(',').count();
    let first = csv.lines().nth(1).unwrap().split(',').count();
    assert_eq!(cols, first);
}

#[test]
fn spec_round_trips_through_json() {
    let s = spec();
    let json = s.to_json();
    let back = EngineSpec::from_json(&json).unwrap();
    assert_eq!(back.name, s.name);
    assert_eq!(back.design_point.n1_100pct_rpm, 5175.0);
    assert_eq!(back.limits.egt_takeoff_c, 950.0);
}
