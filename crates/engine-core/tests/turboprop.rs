//! Validation of the PT6A-114A model against the Cessna 208B POH and P&WC data.

use engine_core::turboprop::{tp_size, TpMode, TurbopropEngine, TurbopropSpec};

const FTLB: f64 = 1.355818;
const PPH: f64 = 7936.64;

fn engine() -> TurbopropEngine {
    TurbopropEngine::new(TurbopropSpec::pt6a_114a())
}

fn run_for(e: &mut TurbopropEngine, seconds: f64) {
    let n = (seconds / 0.05).round() as usize;
    for _ in 0..n {
        e.step(0.05);
    }
}

fn report(label: &str, e: &TurbopropEngine) {
    let s = e.state();
    println!(
        "{label:<36} Ng {:5.1}%  Np {:4.0}  Tq {:4.0} ftlb ({:3.0}%)  ITT {:4.0} C  {:4.0} shp  FF {:4.0} pph  beta {:4.1}  thrust {:4.0} lbf  eta_p {:.2}  SFC {:.2}  oilP {:3.0} T4 {:.0}  mode {:?}",
        s.ng_pct, s.np_rpm, s.torque_ftlb, s.torque_pct, s.itt_c, s.shaft_power_hp, s.fuel_flow_pph, s.prop_beta_deg, s.thrust_lbf, s.prop.efficiency, s.sfc_lb_shp_h, s.oil_pressure_psi, s.t4_k, s.mode
    );
}

#[test]
fn design_point_gives_rated_power() {
    let z = tp_size(&TurbopropSpec::pt6a_114a());
    let r = &z.takeoff;
    println!("sized: A45 {:.5}  T4 {:.0} K  ITT {:.0} C  FF {:.0} pph  shp {:.0}  SFC {:.3}  OPR {:.2}  W2 {:.2}", z.a45_effective, z.t4_takeoff_k, r.tt45 - 273.15, r.wf * PPH, r.shaft_power_w / 745.7, r.wf * PPH / (r.shaft_power_w / 745.7), r.pr, r.w2);
    assert!((r.shaft_power_w / 745.7 - 675.0).abs() < 2.0);
    // POH: takeoff ITT well below the 805 C limit on a standard day (~700-760 C), FF ~ 380-420 pph
    let itt = r.tt45 - 273.15;
    assert!((640.0..=790.0).contains(&itt), "takeoff ITT {:.0} C", itt);
    let ff = r.wf * PPH;
    assert!((340.0..=450.0).contains(&ff), "takeoff fuel flow {:.0} pph", ff);
    let sfc = ff / 675.0;
    assert!((0.50..=0.67).contains(&sfc), "SFC {:.3} lb/shp/h", sfc);
}

#[test]
fn takeoff_static_run() {
    let mut e = engine();
    e.set_running(1.0);
    run_for(&mut e, 15.0);
    report("takeoff, static, SL ISA", &e);
    let s = e.state();
    assert!((1_850.0..=1_915.0).contains(&s.np_rpm), "Np {:.0}", s.np_rpm);
    assert!(s.governing, "prop should be on governor at takeoff");
    // Torque at 675 shp and 1900 rpm = 1866 ft lb (limit 1970)
    assert!((1_700.0..=1_975.0).contains(&s.torque_ftlb), "torque {:.0} ft lb", s.torque_ftlb);
    assert!((2_200.0..=3_200.0).contains(&s.thrust_lbf), "static thrust {:.0} lbf", s.thrust_lbf);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn ground_idle_indications() {
    let mut e = engine();
    e.controls.prop_lever = 1.0;
    e.set_running(0.0);
    run_for(&mut e, 30.0);
    report("low idle, static, prop max", &e);
    let s = e.state();
    // POH: low idle Ng ~ 52 %, ITT ~ 400-500 C, FF ~ 90-120 pph, Np ~ 1,000-1,300 (beta range)
    assert!((50.0..=56.0).contains(&s.ng_pct), "idle Ng {:.1}", s.ng_pct);
    assert!((350.0..=560.0).contains(&s.itt_c), "idle ITT {:.0} C", s.itt_c);
    assert!((70.0..=140.0).contains(&s.fuel_flow_pph), "idle fuel flow {:.0} pph", s.fuel_flow_pph);
    assert!((400.0..=1_450.0).contains(&s.np_rpm), "idle Np {:.0}", s.np_rpm);
    assert!(!s.governing, "prop should be on the fine stop at idle");
    assert!(s.oil_pressure_psi >= 40.0, "idle oil pressure {:.0}", s.oil_pressure_psi);
    e.controls.condition_lever = 2;
    run_for(&mut e, 20.0);
    let hi = e.state();
    assert!((63.0..=68.0).contains(&hi.ng_pct), "high idle Ng {:.1}", hi.ng_pct);
}

#[test]
fn cruise_at_10000_ft() {
    // POH: 10,000 ft, max cruise ~ 1,750-1,865 ft lb at 1,900 rpm (torque limited at low altitude,
    // ITT limited higher), ~ 160 KTAS, FF ~ 330-360 pph.
    let mut e = engine();
    e.environment.altitude_m = 3_048.0;
    e.environment.mach = 0.25; // ~ 160 KTAS
    e.controls.prop_lever = 1.0;
    e.set_running(1.0);
    run_for(&mut e, 30.0);
    report("cruise 10,000 ft, M0.25, full lever", &e);
    let s = e.state();
    assert!((1_150.0..=1_975.0).contains(&s.torque_ftlb), "cruise torque {:.0}", s.torque_ftlb);
    assert!((250.0..=400.0).contains(&s.fuel_flow_pph), "cruise fuel flow {:.0} pph", s.fuel_flow_pph);
    assert!(s.prop.efficiency > 0.75, "prop efficiency {:.2}", s.prop.efficiency);
    assert!(s.itt_c < 805.0, "cruise ITT {:.0}", s.itt_c);
}

#[test]
fn torque_and_itt_limits_with_altitude_and_temperature() {
    // At sea level the engine is torque limited (1970 ft lb) before ITT; hot and high it becomes ITT limited.
    let mut sl = engine();
    sl.controls.prop_lever = 1.0;
    sl.set_running(1.0);
    run_for(&mut sl, 10.0);
    let a = sl.state();
    let mut hot_high = engine();
    hot_high.environment.altitude_m = 2_000.0;
    hot_high.environment.delta_isa_k = 25.0;
    hot_high.controls.prop_lever = 1.0;
    hot_high.set_running(1.0);
    run_for(&mut hot_high, 10.0);
    let b = hot_high.state();
    report("full lever, hot and high (2000 m, ISA+25)", &hot_high);
    assert!(b.torque_ftlb < a.torque_ftlb - 200.0, "hot/high torque {:.0} vs SL {:.0}", b.torque_ftlb, a.torque_ftlb);
    assert!(b.itt_c > a.itt_c + 30.0, "hot/high ITT {:.0} vs SL {:.0}", b.itt_c, a.itt_c);
}

#[test]
fn prop_lever_governs_rpm_and_feather_stops_the_prop() {
    let mut e = engine();
    e.controls.prop_lever = 1.0;
    e.set_running(0.7);
    run_for(&mut e, 10.0);
    let max = e.state();
    e.controls.prop_lever = 0.1; // min governed rpm 1600
    run_for(&mut e, 15.0);
    let min = e.state();
    println!("prop lever: max {:.0} rpm (beta {:.1}), min {:.0} rpm (beta {:.1}), torque {:.0} -> {:.0}", max.np_rpm, max.prop_beta_deg, min.np_rpm, min.prop_beta_deg, max.torque_ftlb, min.torque_ftlb);
    assert!((1_560.0..=1_650.0).contains(&min.np_rpm), "min governed rpm {:.0}", min.np_rpm);
    assert!(min.prop_beta_deg > max.prop_beta_deg + 3.0, "coarser pitch at lower rpm");
    assert!(min.torque_ftlb > max.torque_ftlb, "same power at lower rpm means more torque");
    // Feather in flight with the engine shut down
    e.environment.mach = 0.3;
    e.controls.condition_lever = 0;
    e.controls.prop_lever = 0.0;
    run_for(&mut e, 40.0);
    let f = e.state();
    report("shut down, feathered, M0.3", &e);
    assert!(f.feathered);
    assert!(f.np_rpm < 400.0, "feathered prop still turning at {:.0} rpm", f.np_rpm);
    assert!(f.prop.thrust_n.abs() < 1_500.0, "feathered drag/thrust {:.0} N", f.prop.thrust_n);
}

#[test]
fn reverse_thrust_on_landing_roll() {
    let mut e = engine();
    e.environment.mach = 0.1;
    e.controls.prop_lever = 1.0;
    e.set_running(0.0);
    run_for(&mut e, 5.0);
    e.controls.power_lever = -0.3;
    run_for(&mut e, 10.0);
    report("max reverse, 65 kt", &e);
    let s = e.state();
    assert!(s.prop_beta_deg < 0.0, "beta {:.1}", s.prop_beta_deg);
    assert!(s.thrust_n < -3_000.0, "reverse thrust {:.0} N", s.thrust_n);
    assert!(s.ng_pct <= 89.0, "Ng in reverse {:.1}", s.ng_pct);
}

#[test]
fn ground_start_sequence() {
    let mut e = engine();
    e.controls.prop_lever = 1.0;
    e.controls.starter = true;
    let mut peak_itt: f64 = 0.0;
    let mut fuel_on = None;
    let mut running = None;
    let mut t = 0.0;
    while t < 120.0 {
        let s = e.state();
        if fuel_on.is_none() && s.ng_pct >= 14.0 {
            e.controls.condition_lever = 1;
            fuel_on = Some(t);
        }
        if s.mode == TpMode::Starting { peak_itt = peak_itt.max(s.itt_c) }
        if s.mode == TpMode::Running { running = Some(t); break }
        e.step(0.05);
        t += 0.05;
    }
    let fo = fuel_on.expect("starter should reach 14% Ng");
    let r = running.expect("should reach idle");
    println!("start: fuel on at {:.1} s, idle at {:.1} s, peak ITT {:.0} C", fo, r, peak_itt);
    assert!(fo < 20.0, "fuel on at {:.1}", fo);
    assert!((15.0..=60.0).contains(&(r - fo)), "start took {:.1} s", r - fo);
    assert!((600.0..=1_000.0).contains(&peak_itt), "start ITT peak {:.0} C", peak_itt);
    e.controls.starter = false;
    run_for(&mut e, 20.0);
    let s = e.state();
    assert!((50.0..=56.0).contains(&s.ng_pct), "idle after start {:.1}", s.ng_pct);
}

#[test]
fn weak_battery_hot_start() {
    let mut e = engine();
    e.faults.starter_weak = true;
    e.faults.start_fuel_factor = 1.7;
    e.controls.prop_lever = 1.0;
    e.controls.starter = true;
    run_for(&mut e, 25.0);
    let ng = e.state().ng_pct;
    assert!(ng < 14.0, "weak starter motored to {:.1}%", ng);
    e.controls.condition_lever = 1;
    let mut peak: f64 = 0.0;
    let mut seen_hot = false;
    for _ in 0..1_200 {
        e.step(0.05);
        let s = e.state();
        peak = peak.max(s.itt_c);
        if s.warnings.iter().any(|w| w.starts_with("HOT START")) { seen_hot = true }
    }
    println!("weak battery start: peak ITT {:.0} C", peak);
    assert!(peak > 1_090.0, "expected a hot start, peak only {:.0} C", peak);
    assert!(seen_hot, "hot start warning expected");
}

#[test]
fn flameout_relights_in_flight() {
    let mut e = engine();
    e.environment.altitude_m = 2_500.0;
    e.environment.mach = 0.25;
    e.controls.prop_lever = 1.0;
    e.set_running(0.6);
    run_for(&mut e, 3.0);
    e.faults.trigger_flameout = true;
    e.step(0.05);
    assert!(!e.state().lit);
    e.controls.ignition = true;
    run_for(&mut e, 60.0);
    let s = e.state();
    report("after flameout + ignition, 60 s", &e);
    assert!(s.lit && s.mode == TpMode::Running, "mode {:?} Ng {:.1}", s.mode, s.ng_pct);
}

#[test]
fn prop_governor_failure_overspeeds() {
    let mut e = engine();
    e.controls.prop_lever = 1.0;
    e.set_running(0.5);
    run_for(&mut e, 5.0);
    e.faults.prop_governor_fail = true;
    e.controls.power_lever = 1.0;
    run_for(&mut e, 10.0);
    let s = e.state();
    report("governor failed, lever full", &e);
    assert!(s.np_rpm > 1_950.0, "Np {:.0} with a frozen blade angle at full power", s.np_rpm);
    assert!(s.warnings.iter().any(|w| w.contains("OVERSPEED") || w.contains("above")), "{:?}", s.warnings);
}

#[test]
fn logger_round_trip() {
    let mut e = engine();
    e.controls.prop_lever = 1.0;
    e.set_running(0.5);
    e.logger.set_rate(10.0);
    e.logger.start(e.time());
    run_for(&mut e, 5.0);
    let csv = e.logger.to_csv();
    assert_eq!(csv.lines().next().unwrap().split(',').count(), engine_core::turboprop::TP_LOG_COLUMNS.len());
    assert!((49..=51).contains(&e.logger.len()));
}
