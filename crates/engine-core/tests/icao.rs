//! Takeoff fuel flow against the ICAO Aircraft Engine Emissions Databank
//! (EASA-hosted edition of March 2026), at each record's own conditions: sea
//! level static, ISA, the record's rated thrust and bleed. A failing tolerance
//! here means the preset's calibration moved, not that the databank is wrong;
//! the tolerances state how close each preset currently is.
//!
//! Records: CFM56-7B26 (1.221 kg/s at 117.0 kN), LEAP-1B28 08P28CM143
//! (1.070 kg/s at 130.4 kN), PW1127G-JM Block D 01P22PW163 (0.800 kg/s at
//! 120.4 kN), V2527-A5 1IA003 (1.053 kg/s at 111.2 kN, 4.5 % bleed).

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{size_design_point, steady_point, CycleInput};
use engine_core::spec::interp;
use engine_core::EngineSpec;

/// Fuel flow (kg/s) and N1 (%) at which the sized engine makes `thrust_n` sea level static
fn fuel_at_thrust(s: &EngineSpec, thrust_n: f64, bleed: f64) -> (f64, f64) {
    let d = size_design_point(s);
    let inl = inlet(isa(0.0, 0.0), 0.0);
    let point = |n1: f64| {
        let n2c = interp(&s.spools.n2_vs_n1, n1) / 100.0;
        steady_point(s, &inl, CycleInput { bleed_fraction: bleed, ..CycleInput::steady(n1 / 100.0, n2c, 0.0, None) }, d.core_nozzle_area_m2)
    };
    let (mut lo, mut hi) = (60.0, 104.0);
    for _ in 0..40 {
        let m = 0.5 * (lo + hi);
        if point(m).net_thrust_n < thrust_n { lo = m } else { hi = m }
    }
    let n1 = 0.5 * (lo + hi);
    (point(n1).wf_kg_s, n1)
}

#[test]
fn takeoff_fuel_flow_against_the_icao_databank() {
    // (preset, rated thrust kN, bleed fraction, ICAO fuel flow kg/s, tolerance)
    let cases: [(EngineSpec, f64, f64, f64, f64); 4] = [
        (EngineSpec::cfm56_7b26(), 117.0, 0.0, 1.221, 0.02),
        (EngineSpec::leap_1b28(), 130.4, 0.0, 1.070, 0.03),
        (EngineSpec::pw1127g_jm(), 120.4, 0.0, 0.800, 0.05),
        // Open calibration item: this cycle burns ~16 % more than the record (see the preset's doc comment)
        (EngineSpec::v2527_a5(), 111.2, 0.045, 1.053, 0.18),
    ];
    for (s, kn, bleed, icao, tol) in cases {
        let (wf, n1) = fuel_at_thrust(&s, kn * 1000.0, bleed);
        let err = wf / icao - 1.0;
        println!("{:11} {kn:.1} kN bleed {:.1} %: N1 {n1:.1} %, {wf:.3} kg/s vs ICAO {icao:.3} ({:+.1} %)", s.name, bleed * 100.0, err * 100.0);
        assert!(err.abs() <= tol, "{}: {wf:.3} kg/s vs ICAO {icao:.3} kg/s ({:+.1} %, tolerance {:.0} %)", s.name, err * 100.0, tol * 100.0);
    }
}
