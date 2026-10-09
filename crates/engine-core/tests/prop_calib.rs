//! Propeller calibration search (run with --ignored --nocapture). Uses the
//! real propeller model so the numbers go straight into the specs.
use engine_core::propeller::{prop_forces, PropSpec};

const KT: f64 = 0.514444;

fn equilibrium_rpm(p: &PropSpec, rho: f64, v: f64, beta: f64, power_of_rpm: impl Fn(f64) -> f64) -> f64 {
    let (mut lo, mut hi) = (300.0, 6000.0);
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        let f = prop_forces(p, rho, 340.0, v, mid, beta);
        if f.power_w < power_of_rpm(mid) { lo = mid } else { hi = mid }
    }
    lo
}

#[test]
#[ignore]
fn search_piston_prop() {
    let base = engine_core::piston::PistonSpec::lycoming_o360().propeller;
    let mut best: Option<(f64, f64, f64, f64, f64, f64, f64)> = None;
    let mut beta = 15.0;
    while beta <= 30.0 {
        let mut area = 0.04;
        while area <= 0.40 {
            let p = PropSpec { blade_area_m2: area, fixed_pitch_deg: Some(beta), ..base.clone() };
            let rs = equilibrium_rpm(&p, 1.225, 0.0, beta, |r| 134.2e3 * r / 2700.0);
            let ts = prop_forces(&p, 1.225, 340.0, 0.0, rs, beta).thrust_n;
            let rc = equilibrium_rpm(&p, 0.963, 118.0 * KT, beta, |r| 0.76 * 134.2e3 * r / 2700.0);
            let fc = prop_forces(&p, 0.963, 340.0, 118.0 * KT, rc, beta);
            let err = ((rs - 2350.0) / 100.0).powi(2) + ((rc - 2550.0) / 100.0).powi(2);
            if best.map_or(true, |b| err < b.0) { best = Some((err, beta, area, rs, ts / 4.448, rc, fc.efficiency)); }
            area += 0.005;
        }
        beta += 0.5;
    }
    let b = best.unwrap();
    println!("PISTON beta {:.1} area {:.3} | static rpm {:.0} thrust {:.0} lbf | cruise rpm {:.0} eta {:.2}", b.1, b.2, b.3, b.4, b.5, b.6);
}

#[test]
#[ignore]
fn search_turboprop_prop() {
    let base = engine_core::turboprop::TurbopropSpec::pt6a_114a().propeller;
    for area in [0.30, 0.35, 0.40, 0.45, 0.50] {
        let p = PropSpec { blade_area_m2: area, ..base.clone() };
        // static: beta that absorbs 675 shp at 1900 rpm
        let (mut lo, mut hi) = (0.0, 60.0);
        for _ in 0..50 { let mid = 0.5 * (lo + hi); if prop_forces(&p, 1.225, 340.0, 0.0, 1900.0, mid).power_w < 503e3 { lo = mid } else { hi = mid } }
        let bs = lo; let fs = prop_forces(&p, 1.225, 340.0, 0.0, 1900.0, bs);
        let (mut lo, mut hi) = (0.0, 70.0);
        for _ in 0..50 { let mid = 0.5 * (lo + hi); if prop_forces(&p, 0.905, 328.0, 160.0 * KT, 1900.0, mid).power_w < 358e3 { lo = mid } else { hi = mid } }
        let bc = lo; let fc = prop_forces(&p, 0.905, 328.0, 160.0 * KT, 1900.0, bc);
        // reverse at 65 kt, beta -11: torque sign and thrust
        let fr = prop_forces(&p, 1.225, 340.0, 65.0 * KT, 1900.0, -11.0);
        // fine pitch 6 deg absorbing 8 hp static -> rpm
        let ri = equilibrium_rpm(&p, 1.225, 0.0, 6.0, |_| 6.0e3);
        println!("TURBOPROP area {:.2} | static beta {:.1} thrust {:.0} lbf | cruise beta {:.1} eta {:.2} | reverse: torque {:.0} Nm thrust {:.0} N | idle rpm at 8 hp: {:.0}", area, bs, fs.thrust_n / 4.448, bc, fc.efficiency, fr.torque_nm, fr.thrust_n, ri);
    }
}

#[test]
#[ignore]
fn reverse_debug() {
    let p = engine_core::turboprop::TurbopropSpec::pt6a_114a().propeller;
    for (v, beta) in [(0.0, -11.0), (65.0 * KT, -11.0), (65.0 * KT, -6.0), (65.0 * KT, 0.0), (65.0 * KT, 6.0)] {
        let f = prop_forces(&p, 1.225, 340.0, v, 1900.0, beta);
        println!("v {:5.1} beta {:5.1} | alpha {:6.1} stalled {} thrust {:7.0} N torque {:6.0} Nm power {:6.0} kW", v, beta, f.alpha_deg, f.stalled, f.thrust_n, f.torque_nm, f.power_w / 1000.0);
    }
}
