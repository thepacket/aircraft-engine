//! Validation of the CFM56-7B26 model against published operating points and
//! expected abnormal behaviour.
//!
//! Sources: FAA TCDS E00055EN, Boeing 737NG FCOM (limitations, engine
//! indications, non-normal procedures), CFM56-7B published performance,
//! FAR 33.73 (acceleration), FAR 33.89 (operation / relight envelope).
//! Tolerances are explicit so a student can see what the model is and is not
//! expected to reproduce.

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{size_design_point, steady_point, CycleInput};
use engine_core::spec::interp;
use engine_core::{Engine, EngineSpec, Mode};

const LBF: f64 = 4.448222;
const PPH: f64 = 7936.64; // kg/s -> lb/h

fn spec() -> EngineSpec {
    EngineSpec::cfm56_7b26()
}

fn run_for(e: &mut Engine, seconds: f64) {
    let n = (seconds / 0.05).round() as usize;
    for _ in 0..n {
        e.step(0.05);
    }
}

fn point(n1: f64, alt: f64, mach: f64, disa: f64) -> engine_core::cycle::CycleResult {
    let s = spec();
    let d = size_design_point(&s);
    let inl = inlet(isa(alt, disa), mach);
    let n1c = n1 / inl.theta.sqrt() / 100.0;
    let n2c = interp(&s.spools.n2_vs_n1, n1c * 100.0) / 100.0;
    steady_point(&s, &inl, CycleInput::steady(n1c, n2c, 0.0, None), d.core_nozzle_area_m2)
}

// ---------------------------------------------------------------------------
// Steady-state operating points
// ---------------------------------------------------------------------------

#[test]
fn design_point_reproduces_rated_thrust() {
    let s = spec();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    let err = (r.net_thrust_n - s.rating.takeoff_thrust_n).abs() / s.rating.takeoff_thrust_n;
    assert!(err < 0.002, "thrust {:.0} N vs rated {:.0} N", r.net_thrust_n, s.rating.takeoff_thrust_n);
    assert!(d.core_nozzle_area_m2 > 0.1 && d.core_nozzle_area_m2 < 1.0, "A9 = {:.3} m2", d.core_nozzle_area_m2);
    assert!((r.overall_pressure_ratio - 32.7).abs() < 0.2);
    assert!((r.bypass_ratio - 5.1).abs() < 0.1);
}

#[test]
fn takeoff_point_fuel_flow_and_egt_are_realistic() {
    let s = spec();
    let d = size_design_point(&s);
    let r = &d.takeoff;
    let wf_pph = r.wf_kg_s * PPH;
    assert!((8_500.0..=10_500.0).contains(&wf_pph), "takeoff fuel flow {:.0} lb/h", wf_pph);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    assert!((0.32..=0.40).contains(&tsfc), "takeoff TSFC {:.3} lb/lbf/h", tsfc);
    let egt_c = r.egt_k - 273.15;
    assert!((780.0..=920.0).contains(&egt_c), "takeoff EGT {:.0} C", egt_c);
    assert!((1_500.0..=1_800.0).contains(&d.t4_takeoff_k), "T4 {:.0} K", d.t4_takeoff_k);
    let t3 = r.stations.iter().find(|x| x.id == "3").unwrap().tt_k - 273.15;
    assert!((520.0..=680.0).contains(&t3), "T3 {:.0} C", t3);
}

#[test]
fn ground_idle_matches_cockpit_indications() {
    let r = point(21.0, 0.0, 0.0, 0.0);
    let wf_pph = r.wf_kg_s * PPH;
    let egt_c = r.egt_k - 273.15;
    let thrust_lbf = r.net_thrust_n / LBF;
    assert!((800.0..=1_400.0).contains(&wf_pph), "idle fuel flow {:.0} lb/h", wf_pph);
    assert!((350.0..=500.0).contains(&egt_c), "idle EGT {:.0} C", egt_c);
    assert!((600.0..=2_500.0).contains(&thrust_lbf), "idle thrust {:.0} lbf", thrust_lbf);
}

#[test]
fn cruise_point_is_realistic() {
    let r = point(85.0, 10_668.0, 0.78, 0.0);
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
    for (alt, mach) in [(0.0, 0.0), (0.0, 0.25), (3_000.0, 0.5), (10_668.0, 0.78)] {
        let mut last_thrust = -1.0;
        let mut last_wf = -1.0;
        let mut n1 = 21.0;
        while n1 <= 104.0 {
            let r = point(n1, alt, mach, 0.0);
            assert!(r.t4_k > 400.0 && r.t4_k < 2_500.0, "T4 {:.0} K at N1 {} alt {} M {}", r.t4_k, n1, alt, mach);
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
fn customer_bleed_raises_egt_and_costs_thrust() {
    let s = spec();
    let d = size_design_point(&s);
    let inl = inlet(isa(0.0, 0.0), 0.0);
    let n2c = interp(&s.spools.n2_vs_n1, 100.0) / 100.0;
    let base = steady_point(&s, &inl, CycleInput::steady(1.0, n2c, 0.0, None), d.core_nozzle_area_m2);
    let bled = steady_point(&s, &inl, CycleInput { bleed_fraction: 0.07, ..CycleInput::steady(1.0, n2c, 0.0, None) }, d.core_nozzle_area_m2);
    assert!(bled.egt_k > base.egt_k + 10.0, "EGT with bleed {:.0} vs {:.0} K", bled.egt_k, base.egt_k);
    assert!(bled.net_thrust_n < base.net_thrust_n * 0.99, "thrust with bleed {:.0} vs {:.0} N", bled.net_thrust_n, base.net_thrust_n);
    assert!(bled.bleed_kg_s > 3.0);
}

// ---------------------------------------------------------------------------
// Dynamics: normal operation
// ---------------------------------------------------------------------------

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
    assert!(running - fuel_on > 20.0 && running - fuel_on < 90.0, "start took {:.1} s", running - fuel_on);
    assert!(peak_egt < spec().limits.egt_start_c, "start EGT peaked at {:.0} C", peak_egt);
    assert!(peak_egt > 350.0, "start EGT peak only {:.0} C (no light-off?)", peak_egt);
    run_for(&mut e, 30.0);
    let st = e.state();
    assert!((st.n1_pct - 21.0).abs() < 1.5, "idle N1 {:.1}", st.n1_pct);
    assert!((st.n2_pct - 60.0).abs() < 1.5, "idle N2 {:.1}", st.n2_pct);
    assert!(st.oil_pressure_psi > spec().limits.oil_pressure_min_psi);
    assert!(st.warnings.is_empty(), "warnings at idle: {:?}", st.warnings);
}

#[test]
fn idle_to_takeoff_acceleration_meets_far_33_73() {
    // FAR 33.73: from flight idle to 95% rated takeoff thrust in not more than 5 s.
    // From ground idle (lower) a CFM56-7B takes ~5-7 s; allow 8.
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    run_for(&mut e, 10.0);
    let t0 = e.time();
    e.controls.tla = 1.0;
    let mut t95 = None;
    let mut peak_egt: f64 = 0.0;
    let mut max_ratio: f64 = 0.0;
    let mut min_sm: f64 = 100.0;
    while e.time() - t0 < 25.0 {
        e.step(0.02);
        let st = e.state();
        peak_egt = peak_egt.max(st.egt_c);
        max_ratio = max_ratio.max(st.wf_p3_ratio);
        min_sm = min_sm.min(st.hpc_surge_margin_pct);
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
    assert!(min_sm > 2.0, "surge margin dipped to {:.1}% during a normal acceleration", min_sm);
    assert!(st.surge_count == 0, "surged during a normal acceleration");
    assert!(st.warnings.is_empty(), "warnings during takeoff: {:?}", st.warnings);
    // The accel schedule is what limited the transient: the fuel demand exceeded it
    assert!(max_ratio > 1.0, "Wf/Ps3 peaked at only {:.2}", max_ratio);
}

#[test]
fn takeoff_to_idle_deceleration_is_realistic() {
    let mut e = Engine::new(spec());
    e.set_running(1.0);
    run_for(&mut e, 10.0);
    let t0 = e.time();
    e.controls.tla = 0.0;
    let mut t_idle = None;
    while e.time() - t0 < 30.0 {
        e.step(0.02);
        let st = e.state();
        assert!(st.lit, "flameout during deceleration");
        if t_idle.is_none() && st.n1_pct < 25.0 {
            t_idle = Some(e.time() - t0);
        }
    }
    let t_idle = t_idle.expect("reached idle");
    assert!((4.0..=15.0).contains(&t_idle), "takeoff to idle took {:.1} s", t_idle);
}

#[test]
fn flat_rating_limits_n1_on_a_hot_day() {
    let mut cold = Engine::new(spec());
    cold.environment.delta_isa_k = 10.0;
    cold.set_running(1.0);
    run_for(&mut cold, 15.0);
    let c = cold.state();
    assert_eq!(c.rating_limit, "N1");
    assert!((c.n1_pct - 100.0 * c.inlet.theta.sqrt()).abs() < 1.0, "N1 at ISA+10 {:.1}", c.n1_pct);

    let mut hot = Engine::new(spec());
    hot.environment.delta_isa_k = 30.0;
    hot.set_running(1.0);
    run_for(&mut hot, 15.0);
    let h = hot.state();
    assert_eq!(h.rating_limit, "EGT");
    assert!(h.n1_pct < 100.0 * h.inlet.theta.sqrt() - 1.0, "hot day N1 {:.1} should be reduced", h.n1_pct);
    assert!(h.thrust_n < c.thrust_n, "hot day thrust {:.0} should be below ISA+10 {:.0}", h.thrust_n, c.thrust_n);
    // EGT is held near the rated value, well below the 950 C limit
    assert!(h.egt_c < spec().limits.egt_takeoff_c - 20.0, "hot day EGT {:.0} C", h.egt_c);
    assert!((h.egt_c - c.egt_c).abs() < 60.0, "hot {:.0} vs ISA+10 {:.0} C: flat rating should hold EGT", h.egt_c, c.egt_c);
}

#[test]
fn reverser_produces_reverse_thrust_and_limits_n1() {
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    run_for(&mut e, 5.0);
    e.controls.reverser = true;
    e.controls.tla = 1.0;
    run_for(&mut e, 15.0);
    let st = e.state();
    assert!(st.reverser_position > 0.99);
    assert!(st.thrust_n < -10_000.0, "reverse thrust {:.0} N", st.thrust_n);
    assert!(st.n1_pct <= spec().reverser.max_n1_pct + 1.0, "N1 in reverse {:.1}", st.n1_pct);
    e.controls.reverser = false;
    e.controls.tla = 0.0;
    run_for(&mut e, 15.0);
    assert!(e.state().reverser_position < 0.01);
    assert!(e.state().thrust_n > 0.0);
}

#[test]
fn shutdown_spools_down_and_cools() {
    let mut e = Engine::new(spec());
    e.set_running(0.0);
    run_for(&mut e, 5.0);
    e.controls.fuel_lever = false;
    run_for(&mut e, 120.0);
    let st = e.state();
    assert!(st.n2_pct < 2.0, "N2 {:.1} after 120 s", st.n2_pct);
    assert!(st.n1_pct < 5.0, "N1 {:.1} after 120 s", st.n1_pct);
    assert!(st.fuel_flow_kg_s < 1e-3);
    assert!(st.egt_c < 200.0, "EGT {:.0} C after 120 s", st.egt_c);
    assert_eq!(st.mode, Mode::Off);
}

#[test]
fn altitude_cruise_is_stable_under_governing() {
    let mut e = Engine::new(spec());
    e.environment.altitude_m = 10_668.0;
    e.environment.mach = 0.78;
    e.set_running(0.62);
    run_for(&mut e, 60.0);
    let st = e.state();
    assert_eq!(st.mode, Mode::Running);
    assert!((st.n1_pct - st.n1_command_pct).abs() < 0.5, "N1 {:.1} vs command {:.1}", st.n1_pct, st.n1_command_pct);
    assert!(st.n1_dot_pct_s.abs() < 0.05, "N1 still moving {:.3} %/s", st.n1_dot_pct_s);
    assert!(st.warnings.is_empty(), "{:?}", st.warnings);
}

// ---------------------------------------------------------------------------
// Abnormal starts and failures
// ---------------------------------------------------------------------------

fn start_with_faults(f: impl Fn(&mut Engine)) -> (Engine, f64, Option<f64>, Vec<String>) {
    let mut e = Engine::new(spec());
    f(&mut e);
    e.controls.starter = true;
    let mut peak_egt: f64 = 0.0;
    let mut running_at = None;
    let mut fuel_on = false;
    let mut seen = Vec::new();
    for _ in 0..(150.0 / 0.05) as usize {
        let st = e.state();
        if !fuel_on && (st.n2_pct >= 25.0 || (st.n2_pct >= 17.0 && st.n2_dot_pct_s < 0.05 && e.time() > 20.0)) {
            e.controls.fuel_lever = true;
            fuel_on = true;
        }
        if fuel_on {
            peak_egt = peak_egt.max(st.egt_c);
        }
        for w in &st.warnings {
            if !seen.contains(w) {
                seen.push(w.clone());
            }
        }
        if st.mode == Mode::Running && running_at.is_none() {
            running_at = Some(e.time());
            break;
        }
        e.step(0.05);
    }
    (e, peak_egt, running_at, seen)
}

#[test]
fn hot_start_from_rich_fuel_schedule_exceeds_start_limit() {
    let (_, peak, _, warnings) = start_with_faults(|e| e.faults.start_fuel_factor = 1.8);
    assert!(peak > spec().limits.egt_start_c, "rich start peaked at only {:.0} C", peak);
    assert!(warnings.iter().any(|w| w.starts_with("HOT START")), "{:?}", warnings);
}

#[test]
fn wet_start_with_failed_igniters_has_fuel_but_no_light() {
    let (e, peak, running, warnings) = start_with_faults(|e| e.faults.ignition_fail = true);
    assert!(running.is_none(), "engine should not start without ignition");
    assert!(peak < 100.0, "EGT rose to {:.0} C without ignition", peak);
    assert!(e.state().fuel_flow_kg_s > 0.01, "fuel should be flowing in a wet start");
    assert!(warnings.iter().any(|w| w.starts_with("WET START")), "{:?}", warnings);
}

#[test]
fn hung_start_when_starter_drops_out_early() {
    let (e, _, running, warnings) = start_with_faults(|e| e.faults.starter_early_cutout = true);
    assert!(running.is_none(), "engine should hang below idle, reached idle at {:?}", running);
    let st = e.state();
    assert!(st.lit, "should have lit");
    assert!(st.n2_pct > 20.0 && st.n2_pct < 50.0, "hung at N2 {:.1}", st.n2_pct);
    assert!(warnings.iter().any(|w| w.starts_with("HUNG START")), "{:?}", warnings);
}

#[test]
fn weak_starter_cannot_reach_light_off_speed_quickly() {
    let mut e = Engine::new(spec());
    e.faults.starter_weak = true;
    e.controls.starter = true;
    run_for(&mut e, 40.0);
    let st = e.state();
    assert!(st.n2_pct < 20.0, "weak starter motored to {:.1}%", st.n2_pct);
}

#[test]
fn surge_recovers_and_is_counted() {
    let mut e = Engine::new(spec());
    e.set_running(0.5);
    run_for(&mut e, 5.0);
    let before = e.state().thrust_n;
    e.faults.trigger_surge = true;
    e.step(0.05);
    let during = e.state();
    assert!(during.surge, "surge should be active");
    assert!(during.thrust_n < 0.5 * before, "thrust during surge {:.0} vs {:.0}", during.thrust_n, before);
    assert!(during.warnings.iter().any(|w| w.contains("SURGE")));
    run_for(&mut e, 15.0);
    let after = e.state();
    assert!(!after.surge);
    assert_eq!(after.surge_count, 1);
    assert!(after.lit);
    assert!((after.thrust_n - before).abs() / before < 0.05, "thrust after recovery {:.0} vs {:.0}", after.thrust_n, before);
}

#[test]
fn damaged_compressor_surges_on_a_slam_acceleration() {
    let mut e = Engine::new(spec());
    e.faults.compressor_damage_pct = 14.0;
    e.set_running(0.0);
    run_for(&mut e, 5.0);
    e.controls.tla = 1.0;
    run_for(&mut e, 15.0);
    let st = e.state();
    assert!(st.surge_count >= 1, "a compressor with 14 points less margin should surge on a slam accel (min margin seen {:.1}%)", st.hpc_surge_margin_pct);
}

#[test]
fn surge_cycling_recovers_when_the_lever_is_retarded() {
    // A damaged compressor at full lever surges repeatedly; it must not spiral
    // into nonsense, and it must settle at idle once the lever comes back.
    let mut e = Engine::new(spec());
    e.faults.compressor_damage_pct = 14.0;
    e.set_running(0.0);
    run_for(&mut e, 3.0);
    e.controls.tla = 1.0;
    let mut max_egt: f64 = 0.0;
    for _ in 0..(40.0 / 0.05) as usize {
        e.step(0.05);
        let st = e.state();
        max_egt = max_egt.max(st.egt_gas_c);
        assert!(st.t4_k < 2_500.0, "T4 {:.0} K", st.t4_k);
        assert!(st.n2_pct > 15.0, "core collapsed to N2 {:.1}%", st.n2_pct);
    }
    let st = e.state();
    assert!(st.surge_count >= 1, "expected surges with 14 points of margin lost");
    assert!(st.surge_count <= 20, "{} surges in 40 s", st.surge_count);
    assert!(max_egt < 1_300.0, "EGT peaked at {:.0} C", max_egt);
    e.controls.tla = 0.0;
    run_for(&mut e, 40.0);
    let st = e.state();
    assert_eq!(st.mode, Mode::Running, "mode {:?}, N1 {:.1} N2 {:.1}", st.mode, st.n1_pct, st.n2_pct);
    assert!((st.n1_pct - 21.0).abs() < 2.0, "idle N1 {:.1}", st.n1_pct);
    assert!(!st.surge, "still surging at idle");
    assert!(st.egt_c < 600.0, "idle EGT {:.0} C after surge cycling", st.egt_c);
}

#[test]
fn flameout_in_flight_auto_relights_within_the_envelope() {
    let mut e = Engine::new(spec());
    e.environment.altitude_m = 4_572.0; // 15,000 ft
    e.environment.mach = 0.5;
    e.set_running(0.5);
    run_for(&mut e, 5.0);
    e.faults.trigger_flameout = true;
    e.step(0.05);
    let st = e.state();
    assert!(!st.lit);
    assert_eq!(st.flameout_count, 1);
    // Windmilling core keeps N2 above the light-off minimum at M0.5
    run_for(&mut e, 60.0);
    let st = e.state();
    assert!(st.lit, "should have relit");
    assert_eq!(st.mode, Mode::Running, "mode {:?} N2 {:.1}", st.mode, st.n2_pct);
}

#[test]
fn windmill_restart_needs_enough_airspeed() {
    // Too slow: the core windmills below the light-off speed, no relight
    let mut slow = Engine::new(spec());
    slow.environment.altitude_m = 3_000.0;
    slow.environment.mach = 0.25;
    slow.set_running(0.3);
    run_for(&mut slow, 2.0);
    slow.controls.fuel_lever = false;
    run_for(&mut slow, 60.0);
    slow.controls.fuel_lever = true;
    run_for(&mut slow, 60.0);
    let s = slow.state();
    assert!(!s.lit, "should not light at M0.25 with N2 {:.1}%", s.n2_pct);
    assert!(s.warnings.iter().any(|w| w.starts_with("NO LIGHT")), "{:?}", s.warnings);

    // Fast enough: windmilling N2 above the minimum, relight succeeds
    let mut fast = Engine::new(spec());
    fast.environment.altitude_m = 3_000.0;
    fast.environment.mach = 0.55;
    fast.set_running(0.3);
    run_for(&mut fast, 2.0);
    fast.controls.fuel_lever = false;
    run_for(&mut fast, 60.0);
    fast.controls.fuel_lever = true;
    run_for(&mut fast, 90.0);
    let f = fast.state();
    assert!(f.lit && f.mode == Mode::Running, "windmill start failed: mode {:?} N2 {:.1}", f.mode, f.n2_pct);
}

#[test]
fn no_relight_above_the_altitude_envelope() {
    let mut e = Engine::new(spec());
    e.environment.altitude_m = 11_000.0;
    e.environment.mach = 0.78;
    e.set_running(0.6);
    run_for(&mut e, 2.0);
    e.faults.trigger_flameout = true;
    run_for(&mut e, 40.0);
    let st = e.state();
    assert!(!st.lit, "should not relight above 30,000 ft");
    assert!(st.warnings.iter().any(|w| w.contains("relight altitude")), "{:?}", st.warnings);
}

#[test]
fn oil_leak_leads_to_low_pressure_and_seizure() {
    let mut e = Engine::new(spec());
    e.set_running(0.6);
    e.faults.oil_leak_qt_per_min = 6.0;
    run_for(&mut e, 240.0);
    let st = e.state();
    assert!(st.oil_quantity_qt < 1.0, "oil qty {:.1}", st.oil_quantity_qt);
    assert!(st.oil_pressure_psi < spec().limits.oil_pressure_min_psi, "oil pressure {:.0}", st.oil_pressure_psi);
    run_for(&mut e, 120.0);
    let st = e.state();
    assert!(st.seized, "engine should have seized after {:.0} s without oil pressure", st.oil_starved_s);
    assert!(st.n2_pct < 5.0, "seized engine N2 {:.1}", st.n2_pct);
    assert!(st.warnings.iter().any(|w| w.contains("SEIZED")), "{:?}", st.warnings);
}

#[test]
fn fire_warning_and_bottle_require_fuel_cutoff() {
    let mut e = Engine::new(spec());
    e.set_running(0.4);
    e.faults.fire = true;
    run_for(&mut e, 2.0);
    assert!(e.state().fire_warning);
    e.faults.fire_bottle = true;
    e.step(0.05);
    assert!(e.state().fire_warning, "bottle alone should not extinguish with fuel on");
    assert!(e.state().warnings.iter().any(|w| w.contains("fire persists")));
    e.controls.fuel_lever = false;
    e.faults.fire_bottle = true;
    e.step(0.05);
    assert!(!e.state().fire_warning, "fire should be out after cutoff + bottle");
}

#[test]
fn governor_failure_allows_overspeed() {
    let mut e = Engine::new(spec());
    e.faults.n1_governor_fail = true;
    e.set_running(0.0);
    run_for(&mut e, 3.0);
    e.controls.tla = 1.0;
    run_for(&mut e, 20.0);
    let st = e.state();
    assert!(st.n1_pct > spec().limits.n1_max_pct, "N1 {:.1} with governor failed", st.n1_pct);
    assert!(st.warnings.iter().any(|w| w.contains("OVERSPEED")), "{:?}", st.warnings);
}

#[test]
fn egt_probe_failure_blanks_the_indication() {
    let mut e = Engine::new(spec());
    e.set_running(0.5);
    e.faults.egt_probe_fail = true;
    e.step(0.05);
    let st = e.state();
    assert!(!st.egt_valid);
    assert!(st.egt_gas_c > 400.0, "gas is still hot");
}

// ---------------------------------------------------------------------------
// Infrastructure
// ---------------------------------------------------------------------------

#[test]
fn logger_records_csv_at_requested_rate() {
    let mut e = Engine::new(spec());
    e.set_running(0.5);
    e.logger.set_rate(20.0);
    e.logger.start(e.time());
    run_for(&mut e, 5.0);
    e.logger.stop();
    let n = e.logger.len();
    assert!((99..=101).contains(&n), "{} rows for 5 s at 20 Hz", n);
    let csv = e.logger.to_csv();
    let header = csv.lines().next().unwrap();
    assert!(header.starts_with("time_s,mode,tla"));
    assert_eq!(csv.lines().count(), n + 1);
    assert_eq!(header.split(',').count(), csv.lines().nth(1).unwrap().split(',').count());
    assert_eq!(header.split(',').count(), engine_core::engine::LOG_COLUMNS.len());
}

#[test]
fn spec_round_trips_through_json() {
    let s = spec();
    let json = s.to_json();
    let back = EngineSpec::from_json(&json).unwrap();
    assert_eq!(back.name, s.name);
    assert_eq!(back.design_point.n1_100pct_rpm, 5175.0);
    assert_eq!(back.limits.egt_takeoff_c, 950.0);
    assert_eq!(back.fadec.accel_wf_p3_vs_n2c.len(), s.fadec.accel_wf_p3_vs_n2c.len());
}
