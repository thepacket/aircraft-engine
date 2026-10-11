//! Validation of the Allison 250-C20B turboshaft (Bell 206B) against the
//! flight manual: start, rotor engagement, governed rotor speed, hover power.

use engine_core::turboprop::{TpMode, TurbopropEngine, TurbopropSpec};
use engine_core::{AnyEngine, AnySpec};

fn engine() -> TurbopropEngine {
    TurbopropEngine::new(TurbopropSpec::allison_250_c20b())
}

fn run_for(e: &mut TurbopropEngine, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as usize {
        e.step(0.05);
    }
}

fn report(label: &str, e: &TurbopropEngine) {
    let s = e.state();
    println!("{label:<34} N1 {:5.1}%  Nr {:5.1}%  N2 {:5.0} rpm  TQ {:4.0}%  TOT {:4.0} C  {:4.0} shp  FF {:4.0} pph  oilP {:3.0}  mode {:?}  {:?}", s.ng_pct, s.np_pct, s.pt_rpm, s.torque_pct, s.itt_c, s.shaft_power_hp, s.fuel_flow_pph, s.oil_pressure_psi, s.mode, s.warnings);
}

#[test]
fn preset_is_a_turboshaft_and_round_trips() {
    let spec = AnySpec::default_for("turboshaft").unwrap();
    assert_eq!(spec.kind(), "turboshaft");
    assert_eq!(spec.preset(), "turboshaft");
    let json = spec.to_json();
    assert!(json.contains("\"kind\": \"turboshaft\""));
    let back = AnySpec::from_json(&json).unwrap();
    assert_eq!(back.kind(), "turboshaft");
    let e = AnyEngine::new(back);
    assert_eq!(e.kind(), "turboshaft");
}

#[test]
fn hover_power_at_governed_rotor_speed() {
    // Collective ~0.6: around hover power (~300 shp), Nr held at 100 %
    let mut e = engine();
    e.set_running(0.6);
    run_for(&mut e, 20.0);
    report("hover, collective 0.6", &e);
    let s = e.state();
    assert!((98.0..=102.0).contains(&s.np_pct), "Nr {:.1}%", s.np_pct);
    assert!((50.0..=90.0).contains(&s.torque_pct), "torque {:.0}%", s.torque_pct);
    assert!(s.itt_c < 738.0, "TOT {:.0} C", s.itt_c);
    assert!((85.0..=100.0).contains(&s.ng_pct), "N1 {:.1}%", s.ng_pct);
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    // Full collective: torque at or just over 100 %, TOT below the takeoff limit
    e.controls.collective = 1.0;
    run_for(&mut e, 15.0);
    report("full collective", &e);
    let s = e.state();
    assert!((95.0..=112.0).contains(&s.torque_pct), "torque {:.0}%", s.torque_pct);
    assert!((95.0..=102.0).contains(&s.np_pct), "Nr {:.1}% droop", s.np_pct);
    assert!(s.itt_c < 810.0, "TOT {:.0} C", s.itt_c);
}

#[test]
fn collective_pull_droops_then_recovers() {
    let mut e = engine();
    e.set_running(0.2);
    run_for(&mut e, 15.0);
    e.controls.collective = 0.8;
    let mut nr_min = 200.0f64;
    for _ in 0..200 {
        e.step(0.05);
        nr_min = nr_min.min(e.state().np_pct);
    }
    report("after a quick pull to 0.8", &e);
    let s = e.state();
    assert!(nr_min < 99.0 && nr_min > 90.0, "transient droop to {:.1}%", nr_min);
    assert!((98.0..=102.0).contains(&s.np_pct), "Nr recovered to {:.1}%", s.np_pct);
}

#[test]
fn start_and_rotor_engagement() {
    let mut e = engine();
    e.controls.starter = true;
    let (mut t, mut peak, mut fuel_on, mut running) = (0.0, 0.0f64, None, None);
    while t < 120.0 {
        let s = e.state();
        if fuel_on.is_none() && s.ng_pct >= 15.0 { e.controls.condition_lever = 1; fuel_on = Some(t); }
        if s.mode == TpMode::Starting { peak = peak.max(s.itt_c); }
        if s.mode == TpMode::Running { running = Some(t); break; }
        e.step(0.05); t += 0.05;
    }
    let (f, r) = (fuel_on.expect("motored to 15%"), running.expect("reached idle"));
    println!("start: fuel on at {f:.1} s, idle at {r:.1} s, TOT peak {peak:.0} C");
    assert!(r - f > 15.0 && r - f < 70.0, "start took {:.1} s", r - f);
    assert!(peak > 400.0 && peak < 927.0, "start TOT peak {:.0}", peak);
    e.controls.starter = false;
    run_for(&mut e, 30.0);
    report("ground idle, throttle IDLE", &e);
    let s = e.state();
    assert!((60.0..=67.0).contains(&s.ng_pct), "idle N1 {:.1}", s.ng_pct);
    assert!((40.0..=90.0).contains(&s.np_pct), "rotor at idle Nr {:.1}%", s.np_pct);
    // Throttle to FULL: governor brings the rotor to 100 %
    e.controls.condition_lever = 2;
    run_for(&mut e, 40.0);
    report("throttle FULL, flat pitch", &e);
    let s = e.state();
    assert!((98.0..=102.0).contains(&s.np_pct), "Nr {:.1}%", s.np_pct);
    assert!(s.torque_pct < 35.0, "flat-pitch torque {:.0}%", s.torque_pct);
}

#[test]
fn flameout_with_collective_down_keeps_the_rotor_in_autorotation() {
    let mut e = engine();
    e.environment.altitude_m = 1_500.0;
    e.environment.mach = 0.15; // in forward flight: autorotation needs airflow through the rotor
    e.set_running(0.6);
    run_for(&mut e, 10.0);
    e.faults.trigger_flameout = true;
    e.step(0.05);
    e.controls.collective = 0.05;
    let mut nr_min = 200.0f64;
    for _ in 0..600 { e.step(0.05); nr_min = nr_min.min(e.state().np_pct); }
    report("30 s into autorotation", &e);
    let s = e.state();
    assert!(nr_min > 80.0, "Nr fell to {:.0}% with the collective down", nr_min);
    assert!((88.0..=106.0).contains(&s.np_pct), "autorotation Nr {:.0}%", s.np_pct);
    assert!(s.shaft_power_hp < 30.0, "engine still delivering {:.0} shp", s.shaft_power_hp);
}

#[test]
fn governor_failure_lets_the_rotor_run_away_with_collective() {
    let mut e = engine();
    e.set_running(0.5);
    run_for(&mut e, 15.0);
    e.faults.prop_governor_fail = true;
    e.controls.collective = 0.1;
    run_for(&mut e, 8.0);
    report("governor failed, collective lowered", &e);
    let s = e.state();
    assert!(s.np_pct > 104.0, "Nr should overspeed: {:.1}%", s.np_pct);
    assert!(s.warnings.iter().any(|w| w.contains("GOVERNOR") || w.contains("ROTOR")), "{:?}", s.warnings);
}
