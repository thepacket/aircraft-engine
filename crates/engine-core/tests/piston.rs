//! Validation of the Lycoming O-360-A4M model against the PA-28-181 POH and
//! the Lycoming operator's manual.

use engine_core::piston::{PistonEngine, PistonMode, PistonSpec};

const KT: f64 = 0.514444;

fn engine() -> PistonEngine {
    PistonEngine::new(PistonSpec::lycoming_o360())
}

fn run_for(e: &mut PistonEngine, seconds: f64) {
    let n = (seconds / 0.05).round() as usize;
    for _ in 0..n {
        e.step(0.05);
    }
}

fn report(label: &str, e: &PistonEngine) {
    let s = e.state();
    println!(
        "{label:<34} rpm {:5.0}  MAP {:4.1} inHg  {:5.1} hp ({:3.0}%)  FF {:4.1} gph  F/A {:.4}  EGT {:4.0} F  CHT {:3.0} F  oilP {:3.0}  oilT {:3.0} F  thrust {:4.0} lbf  eta_p {:.2}  BSFC {:.2}",
        s.rpm, s.map_inhg, s.power_hp, s.power_pct, s.fuel_flow_gph, s.fuel_air_ratio, s.egt_f, s.cht_f, s.oil_pressure_psi, s.oil_temperature_f, s.thrust_lbf, s.prop.efficiency, s.bsfc_lb_hp_h
    );
}

#[test]
fn full_throttle_static_run_up() {
    // POH: full-throttle static ~2,300-2,450 rpm with the fixed-pitch prop; ~14 gph full rich
    let mut e = engine();
    e.set_running(1.0);
    run_for(&mut e, 20.0);
    report("static, full throttle, rich", &e);
    let s = e.state();
    assert!((2_250.0..=2_480.0).contains(&s.rpm), "static rpm {:.0}", s.rpm);
    assert!((140.0..=175.0).contains(&s.power_hp), "static power {:.0} hp", s.power_hp);
    assert!((12.0..=16.5).contains(&s.fuel_flow_gph), "fuel flow {:.1} gph", s.fuel_flow_gph);
    assert!((1_200.0..=1_400.0).contains(&s.egt_f), "EGT {:.0} F", s.egt_f);
    assert!((330.0..=700.0).contains(&s.thrust_lbf), "static thrust {:.0} lbf", s.thrust_lbf);
}

#[test]
fn rated_power_at_rated_rpm() {
    // 180 hp at 2700 rpm, sea level, full throttle. With the fixed-pitch prop the
    // engine only reaches 2700 rpm in a dive, so check the cycle at 2700 directly
    // by running at high airspeed.
    let mut e = engine();
    e.environment.tas_m_s = 125.0 * KT;
    e.set_running(1.0);
    run_for(&mut e, 20.0);
    report("full throttle, 125 kt (near 2700)", &e);
    let s = e.state();
    assert!((2_600.0..=2_800.0).contains(&s.rpm), "rpm {:.0}", s.rpm);
    assert!((160.0..=195.0).contains(&s.power_hp), "power {:.0} hp at {:.0} rpm", s.power_hp, s.rpm);
}

#[test]
fn cruise_75_percent_at_8000_ft() {
    // POH: 8,000 ft, full throttle ~ 2,500-2,600 rpm, ~75 % power, 115-120 KTAS,
    // ~9-10 gph leaned to best power
    let mut e = engine();
    e.environment.altitude_m = 2_438.0;
    e.environment.tas_m_s = 118.0 * KT;
    e.set_running(1.0);
    run_for(&mut e, 20.0);
    report("8000 ft, full throttle, full rich", &e);
    let rich = e.state();
    assert!((2_450.0..=2_700.0).contains(&rich.rpm), "rpm {:.0}", rich.rpm);
    // Lean to best power (~ F/A 0.08)
    e.controls.mixture = 0.55;
    run_for(&mut e, 20.0);
    report("8000 ft, leaned to best power", &e);
    let lean = e.state();
    assert!((65.0..=85.0).contains(&lean.power_pct), "cruise power {:.0}%", lean.power_pct);
    assert!((8.5..=11.5).contains(&lean.fuel_flow_gph), "cruise fuel flow {:.1} gph", lean.fuel_flow_gph);
    assert!(lean.fuel_flow_gph < rich.fuel_flow_gph - 1.0, "leaning should cut fuel flow: {:.1} vs {:.1}", lean.fuel_flow_gph, rich.fuel_flow_gph);
    assert!(lean.egt_f > rich.egt_f + 50.0, "leaning should raise EGT: {:.0} vs {:.0}", lean.egt_f, rich.egt_f);
    assert!((0.40..=0.55).contains(&lean.bsfc_lb_hp_h), "BSFC {:.2}", lean.bsfc_lb_hp_h);
}

#[test]
fn leaning_past_peak_egt_reduces_power_and_egt() {
    let mut e = engine();
    e.environment.altitude_m = 2_438.0;
    e.environment.tas_m_s = 118.0 * KT;
    e.set_running(1.0);
    let mut peak_egt: f64 = 0.0;
    let mut mix_at_peak = 1.0;
    let mut m = 1.0;
    while m > 0.1 {
        e.controls.mixture = m;
        run_for(&mut e, 12.0);
        let s = e.state();
        if s.egt_f > peak_egt {
            peak_egt = s.egt_f;
            mix_at_peak = m;
        }
        m -= 0.05;
    }
    println!("peak EGT {:.0} F at mixture {:.2}", peak_egt, mix_at_peak);
    assert!((1_400.0..=1_560.0).contains(&peak_egt), "peak EGT {:.0} F", peak_egt);
    assert!(mix_at_peak < 0.9 && mix_at_peak >= 0.25, "peak at mixture {:.2}", mix_at_peak);
    // Lean of peak: cooler and less power; too lean: quits
    e.controls.mixture = mix_at_peak - 0.12;
    run_for(&mut e, 15.0);
    let lop = e.state();
    assert!(lop.egt_f < peak_egt - 40.0, "LOP EGT {:.0} vs peak {:.0}", lop.egt_f, peak_egt);
    assert!(lop.mode == PistonMode::Running);
    e.controls.mixture = 0.0;
    run_for(&mut e, 15.0);
    assert!(e.state().mode != PistonMode::Running, "idle cutoff should stop the engine");
}

#[test]
fn idle_indications() {
    let mut e = engine();
    e.set_running(0.0);
    run_for(&mut e, 30.0);
    report("idle, static", &e);
    let s = e.state();
    assert!((550.0..=800.0).contains(&s.rpm), "idle rpm {:.0}", s.rpm);
    assert!((8.0..=13.0).contains(&s.map_inhg), "idle MAP {:.1}", s.map_inhg);
    assert!((0.6..=2.0).contains(&s.fuel_flow_gph), "idle fuel flow {:.1}", s.fuel_flow_gph);
    assert!(s.oil_pressure_psi >= 25.0, "idle oil pressure {:.0}", s.oil_pressure_psi);
}

#[test]
fn cold_start_needs_prime_and_magnetos() {
    let mut e = engine();
    e.controls.magnetos = 3;
    e.controls.starter = true;
    run_for(&mut e, 8.0);
    let s = e.state();
    assert_eq!(s.mode, PistonMode::Cranking, "unprimed cold engine should crank, not fire");
    assert!((100.0..=250.0).contains(&s.rpm), "cranking rpm {:.0}", s.rpm);
    assert!(s.warnings.iter().any(|w| w.contains("not primed")), "{:?}", s.warnings);
    e.controls.primer_shots = 3;
    let t0 = e.time();
    let mut started = None;
    while e.time() - t0 < 10.0 {
        e.step(0.05);
        if e.state().mode == PistonMode::Running { started = Some(e.time() - t0); break; }
    }
    let t = started.expect("primed engine should start");
    assert!(t < 5.0, "start took {:.1} s", t);
    e.controls.starter = false;
    run_for(&mut e, 20.0);
    report("after cold start, idle", &e);
    let s = e.state();
    assert!((500.0..=900.0).contains(&s.rpm), "idle after start {:.0}", s.rpm);
    assert!(s.cht_f < 150.0, "CHT should still be cold: {:.0} F", s.cht_f);
    assert!(s.oil_pressure_psi > 55.0, "cold oil pressure should be high: {:.0}", s.oil_pressure_psi);
}

#[test]
fn magneto_check_drops() {
    let mut e = engine();
    e.set_running(0.55);
    // Set throttle for ~1800 rpm
    for _ in 0..40 {
        run_for(&mut e, 1.0);
        let r = e.state().rpm;
        e.controls.throttle += (1_800.0 - r) * 0.0002;
    }
    let both = e.state().rpm;
    assert!((1_700.0..=1_900.0).contains(&both), "run-up rpm {:.0}", both);
    e.controls.magnetos = 1;
    run_for(&mut e, 6.0);
    let left = e.state().rpm;
    e.controls.magnetos = 2;
    run_for(&mut e, 6.0);
    let right = e.state().rpm;
    e.controls.magnetos = 3;
    run_for(&mut e, 6.0);
    println!("mag check: both {:.0}, L {:.0} (drop {:.0}), R {:.0} (drop {:.0})", both, left, both - left, right, both - right);
    assert!((40.0..=175.0).contains(&(both - left)), "left drop {:.0}", both - left);
    assert!((40.0..=175.0).contains(&(both - right)), "right drop {:.0}", both - right);
    // Fouled plug: large drop and roughness on the affected mag
    e.faults.plug_fouled = true;
    e.controls.magnetos = 1;
    run_for(&mut e, 6.0);
    let s = e.state();
    assert!(both - s.rpm > 200.0, "fouled plug drop only {:.0}", both - s.rpm);
    assert!(s.roughness > 0.4);
    // Dead magneto: engine quits on that mag alone
    e.controls.magnetos = 3;
    e.faults.plug_fouled = false;
    e.faults.mag_right_fail = true;
    run_for(&mut e, 6.0);
    e.controls.magnetos = 2;
    run_for(&mut e, 8.0);
    assert!(e.state().mode != PistonMode::Running, "engine should quit on a dead magneto");
}

#[test]
fn carburettor_ice_forms_and_clears_with_carb_heat() {
    let mut e = engine();
    e.environment.altitude_m = 1_000.0;
    e.environment.tas_m_s = 100.0 * KT;
    e.environment.delta_isa_k = 2.0; // OAT ~ 10 C
    e.environment.humidity = 0.85;
    e.set_running(0.6);
    run_for(&mut e, 10.0);
    let before = e.state();
    run_for(&mut e, 240.0);
    let iced = e.state();
    report("after 4 min in icing conditions", &e);
    assert!(iced.carb_ice > 0.3, "ice {:.2}", iced.carb_ice);
    assert!(iced.rpm < before.rpm - 100.0, "rpm {:.0} -> {:.0}", before.rpm, iced.rpm);
    assert!(iced.map_inhg < before.map_inhg - 1.0, "MAP {:.1} -> {:.1}", before.map_inhg, iced.map_inhg);
    assert!(iced.warnings.iter().any(|w| w.contains("CARB ICE")), "{:?}", iced.warnings);
    e.controls.carb_heat = true;
    run_for(&mut e, 60.0);
    let cleared = e.state();
    report("after 60 s carb heat", &e);
    assert!(cleared.carb_ice < 0.05, "ice {:.2} after carb heat", cleared.carb_ice);
    assert!(cleared.rpm > iced.rpm + 80.0, "rpm should recover: {:.0} -> {:.0}", iced.rpm, cleared.rpm);
    // Carb heat on a clean engine costs a little power (hotter, thinner air)
    assert!(cleared.rpm < before.rpm, "carb heat should cost a little rpm: {:.0} vs {:.0}", cleared.rpm, before.rpm);
}

#[test]
fn fuel_selector_off_starves_the_engine() {
    let mut e = engine();
    e.set_running(0.7);
    run_for(&mut e, 5.0);
    e.controls.fuel_selector = 0;
    let mut quit_at = None;
    let t0 = e.time();
    while e.time() - t0 < 30.0 {
        e.step(0.05);
        if e.state().mode != PistonMode::Running { quit_at = Some(e.time() - t0); break; }
    }
    let t = quit_at.expect("engine should quit");
    assert!((3.0..=15.0).contains(&t), "quit after {:.1} s", t);
    e.controls.fuel_selector = 3;
    e.controls.starter = true;
    run_for(&mut e, 10.0);
    assert_eq!(e.state().mode, PistonMode::Running, "warm engine restarts with fuel back");
}

#[test]
fn temperatures_settle_in_climb_and_cruise() {
    let mut e = engine();
    e.environment.tas_m_s = 75.0 * KT;
    e.set_running(1.0);
    run_for(&mut e, 600.0);
    report("full power climb at 75 kt, 10 min", &e);
    let climb = e.state();
    assert!((360.0..=460.0).contains(&climb.cht_f), "climb CHT {:.0} F", climb.cht_f);
    assert!((170.0..=235.0).contains(&climb.oil_temperature_f), "climb oil temp {:.0} F", climb.oil_temperature_f);
    assert!(climb.warnings.is_empty(), "{:?}", climb.warnings);
    e.environment.altitude_m = 2_438.0;
    e.environment.tas_m_s = 118.0 * KT;
    e.controls.mixture = 0.55;
    run_for(&mut e, 600.0);
    report("cruise 8000 ft, 118 kt, 10 min", &e);
    let cruise = e.state();
    assert!((300.0..=400.0).contains(&cruise.cht_f), "cruise CHT {:.0} F", cruise.cht_f);
    assert!(cruise.cht_f < climb.cht_f - 20.0, "cruise should be cooler than climb");
}

#[test]
fn logger_round_trip() {
    let mut e = engine();
    e.set_running(0.5);
    e.logger.set_rate(10.0);
    e.logger.start(e.time());
    run_for(&mut e, 5.0);
    e.logger.stop();
    let csv = e.logger.to_csv();
    let header = csv.lines().next().unwrap();
    assert_eq!(header.split(',').count(), engine_core::piston::PISTON_LOG_COLUMNS.len());
    assert!((49..=51).contains(&e.logger.len()));
}
