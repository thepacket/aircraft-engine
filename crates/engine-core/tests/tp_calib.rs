use engine_core::atmosphere::{inlet, isa};
use engine_core::turboprop::{tp_cycle, tp_size, tp_solve_t4, TurbopropSpec};

#[test]
#[ignore]
fn print_steady_points() {
    let s = TurbopropSpec::pt6a_114a();
    let z = tp_size(&s);
    println!("A45 {:.5} T4to {:.0}", z.a45_effective, z.t4_takeoff_k);
    for (ng, alt, mach, disa) in [(52.0, 0.0, 0.0, 0.0), (65.0, 0.0, 0.0, 0.0), (80.0, 0.0, 0.0, 0.0), (90.0, 0.0, 0.0, 0.0), (100.0, 0.0, 0.0, 0.0), (96.0, 3048.0, 0.25, 0.0), (100.0, 2000.0, 0.0, 25.0)] {
        let inl = inlet(isa(alt, disa), mach);
        let ngc = ng / inl.theta.sqrt() / 100.0;
        let npf = if ng < 60.0 { 0.6 } else { 1.0 };
        let t4 = tp_solve_t4(&s, &inl, ngc, z.a45_effective, npf, 0.0);
        let r = tp_cycle(&s, &inl, ngc, t4, true, npf, 0.0);
        println!("Ng {:5.1} alt {:5.0} M {:.2} dISA {:+3.0} | T4 {:5.0} K  ITT {:4.0} C  T3 {:4.0} K  PR {:4.2}  W2 {:.2}  FF {:4.0} pph  Wf/P3 {:.3}  shp {:4.0}  Tq@1900 {:4.0} ftlb  jet {:3.0} lbf  SFC {:.2}",
            ng, alt, mach, disa, r.t4, r.tt45 - 273.15, r.tt3, r.pr, r.w2, r.wf * 7936.64, r.wf * 3600.0 / r.ps3_kpa, r.shaft_power_w / 745.7, r.shaft_power_w / (1900.0 / 60.0 * std::f64::consts::TAU) / 1.355818, r.jet_thrust_n / 4.448, r.wf * 7936.64 / (r.shaft_power_w / 745.7).max(1.0));
    }
}
