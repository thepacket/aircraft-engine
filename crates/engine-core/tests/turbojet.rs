//! Validation of the J85-GE-5A model against the T-38A flight manual and GE data.

use engine_core::turbojet::{tj_cycle, tj_size, tj_solve_t4, TjMode, TurbojetEngine, TurbojetSpec};
use engine_core::atmosphere::{inlet, isa};

const LBF: f64 = 4.448222;
const PPH: f64 = 7936.64;

fn engine() -> TurbojetEngine {
    TurbojetEngine::new(TurbojetSpec::j85_ge_5a())
}

fn run_for(e: &mut TurbojetEngine, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as usize {
        e.step(0.05);
    }
}

fn report(label: &str, e: &TurbojetEngine) {
    let s = e.state();
    println!(
        "{label:<36} N {:5.1}%  EGT {:4.0} C  thrust {:5.0} lbf  FF {:5.0} pph (AB {:4.0})  TSFC {:.2}  nozzle {:3.0}%  AB {}  T4 {:.0}  sm {:4.1}  mode {:?}  {:?}",
        s.n_pct, s.egt_c, s.thrust_lbf, s.fuel_flow_pph, s.ab_fuel_flow_pph, s.tsfc_lb_lbf_h, s.nozzle_pct, s.ab_lit, s.t4_k, s.surge_margin_pct, s.mode, s.warnings
    );
}

#[test]
fn sizing_reproduces_dry_and_afterburning_ratings() {
    let s = TurbojetSpec::j85_ge_5a();
    let z = tj_size(&s);
    let m = &z.military;
    let a = &z.max_ab;
    println!("sized: A8 mil {:.4} m2  A8 AB {:.4} m2  T4 {:.0} K  EGT {:.0} C  mil FF {:.0} pph TSFC {:.3}  AB FF {:.0} pph TSFC {:.3}  OPR {:.2}  W2 {:.1}  V9 {:.0}/{:.0}  NPR {:.2}",
        z.a8_mil_m2, z.a8_ab_max_m2, z.t4_mil_k, m.tt5 - 273.15, m.wf * PPH, m.tsfc * 3600.0 * 9.80665, (a.wf + a.wf_ab) * PPH, a.tsfc * 3600.0 * 9.80665, m.pr, m.w2, m.v9, a.v9, m.npr);
    assert!((m.net_thrust_n - s.rating.thrust_dry_n).abs() / s.rating.thrust_dry_n < 0.002);
    assert!((a.net_thrust_n - s.rating.thrust_ab_n).abs() / s.rating.thrust_ab_n < 0.005);
    // GE: dry TSFC ~1.0, AB ~2.2 lb/lbf/h; EGT at military below the 690 C limit
    let tsfc_dry = m.tsfc * 3600.0 * 9.80665;
    let tsfc_ab = a.tsfc * 3600.0 * 9.80665;
    assert!((0.90..=1.15).contains(&tsfc_dry), "dry TSFC {:.3}", tsfc_dry);
    assert!((1.9..=2.5).contains(&tsfc_ab), "AB TSFC {:.3}", tsfc_ab);
    let egt = m.tt5 - 273.15;
    assert!((580.0..=690.0).contains(&egt), "military EGT {:.0} C", egt);
    assert!(z.a8_ab_max_m2 > z.a8_mil_m2 * 1.2, "AB nozzle opening {:.2}x", z.a8_ab_max_m2 / z.a8_mil_m2);
}

#[test]
fn steady_fuel_ratios_sit_between_the_schedules() {
    let s = TurbojetSpec::j85_ge_5a();
    let z = tj_size(&s);
    let inl = inlet(isa(0.0, 0.0), 0.0);
    for (n, a8) in [(0.48, z.a8_mil_m2 * s.control.nozzle_idle_frac), (0.7, z.a8_mil_m2 * 1.2), (0.85, z.a8_mil_m2 * 1.07), (1.0, z.a8_mil_m2)] {
        let t4 = tj_solve_t4(&s, &inl, n, 0.0, a8, 0.0);
        let r = tj_cycle(&s, &inl, n, t4, true, 0.0, a8, 0.0);
        let ratio = r.wf * 3600.0 / r.ps3_kpa;
        let acc = engine_core::spec::interp(&s.control.accel_wf_p3_vs_nc, n * 100.0);
        let dec = engine_core::spec::interp(&s.control.decel_wf_p3_vs_nc, n * 100.0);
        println!("N {:.0}%: T4 {:.0} K  EGT {:.0} C  FF {:.0} pph  Ps3 {:.0} kPa  Wf/Ps3 {:.2} (decel {:.2}, accel {:.2})  thrust {:.0} lbf", n * 100.0, t4, r.tt5 - 273.15, r.wf * PPH, r.ps3_kpa, ratio, dec, acc, r.net_thrust_n / LBF);
        assert!(dec < ratio && ratio < acc, "N {:.0}%: steady Wf/Ps3 {:.2} outside [{:.2}, {:.2}]", n * 100.0, ratio, dec, acc);
    }
}

#[test]
fn idle_indications() {
    let mut e = engine();
    e.set_running(0.0);
    run_for(&mut e, 20.0);
    report("idle, static, SL ISA", &e);
    let s = e.state();
    // T-38 flight manual: idle ~48-50 % rpm, EGT ~ 350-500 C, ~ 350-450 pph
    assert!((46.0..=51.0).contains(&s.n_pct), "idle N {:.1}", s.n_pct);
    assert!((300.0..=520.0).contains(&s.egt_c), "idle EGT {:.0}", s.egt_c);
    assert!((250.0..=550.0).contains(&s.fuel_flow_pph), "idle FF {:.0}", s.fuel_flow_pph);
    assert!(s.thrust_lbf > 100.0 && s.thrust_lbf < 500.0, "idle thrust {:.0}", s.thrust_lbf);
    assert!(s.nozzle_pct > 50.0, "nozzle should be open at idle: {:.0}%", s.nozzle_pct);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn military_and_afterburner_static() {
    let mut e = engine();
    e.set_running(1.0);
    run_for(&mut e, 15.0);
    report("military, static", &e);
    let s = e.state();
    assert!((99.0..=100.6).contains(&s.n_pct), "mil N {:.1}", s.n_pct);
    assert!((2_550.0..=2_720.0).contains(&s.thrust_lbf), "mil thrust {:.0}", s.thrust_lbf);
    assert!(s.egt_c < 690.0, "mil EGT {:.0}", s.egt_c);
    assert!(s.nozzle_pct < 5.0, "nozzle closed at mil: {:.0}%", s.nozzle_pct);
    e.controls.throttle = 1.3;
    run_for(&mut e, 10.0);
    report("max afterburner, static", &e);
    let s = e.state();
    assert!(s.ab_lit, "afterburner should be lit");
    assert!((3_650.0..=3_950.0).contains(&s.thrust_lbf), "AB thrust {:.0}", s.thrust_lbf);
    assert!(s.nozzle_pct > 60.0, "nozzle open in AB: {:.0}%", s.nozzle_pct);
    assert!((99.0..=100.6).contains(&s.n_pct), "N with AB {:.1} (AB must not change the turbine point)", s.n_pct);
    assert!(s.egt_c < 735.0, "EGT with AB {:.0}", s.egt_c);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    e.controls.throttle = 1.0;
    run_for(&mut e, 6.0);
    let s = e.state();
    assert!(!s.ab_lit && s.thrust_lbf < 2_800.0, "AB cancelled: {:.0} lbf", s.thrust_lbf);
}

#[test]
fn ground_start_and_acceleration() {
    let mut e = engine();
    e.controls.starter = true;
    let (mut t, mut peak, mut fuel_on, mut running) = (0.0, 0.0f64, None, None);
    while t < 120.0 {
        let s = e.state();
        if fuel_on.is_none() && s.n_pct >= 14.0 { e.controls.fuel = true; fuel_on = Some(t); }
        if s.mode == TjMode::Starting { peak = peak.max(s.egt_c); }
        if s.mode == TjMode::Running { running = Some(t); break; }
        e.step(0.05); t += 0.05;
    }
    let (f, r) = (fuel_on.expect("motored to 14%"), running.expect("reached idle"));
    println!("start: fuel on at {f:.1} s, idle at {r:.1} s, EGT peak {peak:.0} C");
    assert!(r - f > 15.0 && r - f < 60.0, "start took {:.1} s", r - f);
    assert!(peak > 300.0 && peak < 760.0, "start EGT peak {:.0}", peak);
    e.controls.starter = false;
    run_for(&mut e, 10.0);
    let t0 = e.time();
    e.controls.throttle = 1.0;
    let (mut t95, mut egt_peak, mut sm_min) = (None, 0.0f64, 100.0f64);
    while e.time() - t0 < 20.0 {
        e.step(0.02);
        let s = e.state();
        egt_peak = egt_peak.max(s.egt_c); sm_min = sm_min.min(s.surge_margin_pct);
        if t95.is_none() && s.thrust_n >= 0.95 * 2_680.0 * LBF { t95 = Some(e.time() - t0); }
    }
    let t95 = t95.expect("95% military thrust");
    report("after idle-to-mil", &e);
    println!("accel: idle to 95% mil thrust in {t95:.1} s, EGT peak {egt_peak:.0}, min stall margin {sm_min:.1}");
    // T-38: idle to military in ~6-8 s on a small spool
    assert!((3.0..=9.0).contains(&t95), "idle to 95% took {:.1} s", t95);
    assert!(egt_peak < 735.0, "EGT overshoot {:.0}", egt_peak);
    assert!(sm_min > 2.0 && e.state().surge_count == 0, "stall margin {:.1}", sm_min);
}

#[test]
fn stuck_closed_nozzle_with_afterburner_over_temperatures_the_turbine() {
    let mut e = engine();
    e.set_running(1.0);
    run_for(&mut e, 10.0);
    e.faults.nozzle_stuck = true;
    e.controls.throttle = 1.3;
    run_for(&mut e, 8.0);
    report("AB with nozzle stuck at mil", &e);
    let s = e.state();
    let ok = engine_state_with_working_nozzle();
    assert!(s.egt_c > ok + 40.0 || s.surge_count > 0, "stuck nozzle: EGT {:.0} vs {:.0} with a working nozzle, surges {}", s.egt_c, ok, s.surge_count);
}

fn engine_state_with_working_nozzle() -> f64 {
    let mut e = engine();
    e.set_running(1.0);
    run_for(&mut e, 10.0);
    e.controls.throttle = 1.3;
    run_for(&mut e, 8.0);
    e.state().egt_c
}

#[test]
fn afterburner_at_altitude_and_throttle_chop() {
    let mut e = engine();
    e.environment.altitude_m = 9_144.0;
    e.environment.mach = 0.85;
    e.set_running(1.0);
    run_for(&mut e, 10.0);
    let mil = e.state().thrust_lbf;
    e.controls.throttle = 1.3;
    run_for(&mut e, 12.0);
    report("max A/B at FL300 M0.85", &e);
    let s = e.state();
    assert!(s.ab_lit, "A/B should light at altitude");
    assert!(s.thrust_lbf > mil * 1.3, "A/B thrust {:.0} vs mil {:.0} lbf", s.thrust_lbf, mil);
    // Corrected-speed topping at altitude: 100 % corrected is ~95 % physical rpm here
    assert!((92.0..=100.6).contains(&s.n_pct), "N {:.1}", s.n_pct);
    assert!(s.surge_count == 0 && s.flameout_count == 0, "stalls {} flameouts {}", s.surge_count, s.flameout_count);
    // Chop to idle from max A/B: the nozzle opens, the spool must not overspeed
    e.controls.throttle = 0.0;
    let mut n_max = 0.0f64;
    for i in 0..400 {
        e.step(0.05); n_max = n_max.max(e.state().n_pct);
        if i % 40 == 0 { let s = e.state(); println!("  chop+{:4.1}s N {:5.1} cmd {:5.1} wf {:5.0} min {:5.0} max {:5.0} dem {:5.0} T4 {:4.0} T4ss {:4.0} A8 {:3.0}% ps3 {:4.0}", (i as f64) * 0.05, s.n_pct, s.n_command_pct, s.fuel_flow_pph, s.fuel_decel_limit_kg_s * PPH, s.fuel_accel_limit_kg_s * PPH, s.fuel_command_kg_s * PPH, s.t4_k, s.t4_steady_k, s.nozzle_pct, s.ps3_kpa); }
    }
    report("20 s after the chop", &e);
    let s = e.state();
    assert!(n_max < 101.5, "overspeed to {:.1}% after the chop", n_max);
    // Flight idle at M0.85 is high: ram pressure through a wide-open nozzle keeps
    // the spool turning fast on the deceleration fuel floor
    assert!(s.n_pct < 90.0, "N {:.1}% 20 s after the chop (flight idle)", s.n_pct);
}

#[test]
fn altitude_cruise_and_steady_helper() {
    let s = TurbojetSpec::j85_ge_5a();
    let z = tj_size(&s);
    let inl = inlet(isa(9_144.0, 0.0), 0.85);
    let nc = 0.97 / inl.theta.sqrt();
    let t4 = tj_solve_t4(&s, &inl, nc, 0.0, z.a8_mil_m2, 0.0);
    let r = tj_cycle(&s, &inl, nc, t4, true, 0.0, z.a8_mil_m2, 0.0);
    let tsfc = r.tsfc * 3600.0 * 9.80665;
    println!("30,000 ft M0.85, 97% N: thrust {:.0} lbf  TSFC {:.2}  EGT {:.0} C", r.net_thrust_n / LBF, tsfc, r.tt5 - 273.15);
    assert!((700.0..=1_500.0).contains(&(r.net_thrust_n / LBF)), "cruise thrust {:.0} lbf", r.net_thrust_n / LBF);
    assert!((0.9..=1.4).contains(&tsfc), "cruise TSFC {:.2}", tsfc);
    use engine_core::{AnyEngine, AnySpec};
    let e = AnyEngine::new(AnySpec::default_for("turbojet").unwrap());
    let v: serde_json::Value = serde_json::from_str(&e.steady_state_json(1.0, -1.0, 0.0, 0.0, 0.0, 0.4, 20.0)).unwrap();
    let thrust = v["thrust_n"].as_f64().unwrap();
    assert!((thrust - s.rating.thrust_dry_n).abs() / s.rating.thrust_dry_n < 0.03, "steady mil thrust {:.0} N", thrust);
}
