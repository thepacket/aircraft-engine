//! Conservation and consistency checks of the core models: mass, energy and
//! fuel bookkeeping, the sign of unpowered (windmilling) thrust and torque,
//! and control behaviour that must not depend on history.

use engine_core::atmosphere::{inlet, isa};
use engine_core::cycle::{burner_fuel, burner_t_out, cycle, size_design_point, steady_point, CycleInput, CP_AIR, CP_GAS, T_REF};
use engine_core::piston::{PistonEngine, PistonSpec};
use engine_core::propeller::prop_forces;
use engine_core::spec::interp;
use engine_core::turbojet::{tj_cycle, tj_size, TurbojetSpec};
use engine_core::turboprop::{tp_cycle, tp_size, TurbopropSpec};
use engine_core::{Engine, EngineSpec};

fn turbofans() -> Vec<EngineSpec> {
    vec![EngineSpec::cfm56_7b26(), EngineSpec::leap_1b28(), EngineSpec::v2527_a5(), EngineSpec::pw1127g_jm()]
}

fn run(e: &mut Engine, seconds: f64) {
    for _ in 0..(seconds / 0.05).round() as usize {
        e.step(0.05);
    }
}

#[test]
fn burner_balance_conserves_energy_from_a_common_datum() {
    let (w, t3, t4, eta, lhv) = (50.0, 800.0, 1_650.0, 0.995, 43.0e6);
    let wf = burner_fuel(w, CP_AIR, t3, t4, eta, lhv);
    let h_in = w * CP_AIR * (t3 - T_REF) + wf * eta * lhv;
    let h_out = (w + wf) * CP_GAS * (t4 - T_REF);
    assert!((h_in - h_out).abs() / h_out < 1e-9, "residual {:.1} W", h_in - h_out);
    let back = burner_t_out(w, CP_AIR, t3, wf, eta, lhv, 2_400.0);
    assert!((back - t4).abs() < 1e-6, "round trip {:.6} K", back);
}

#[test]
fn burner_turbine_and_cooling_mix_conserve_energy() {
    for s in turbofans() {
        let d = size_design_point(&s);
        let r = &d.takeoff;
        let dp = &s.design_point;
        let t = |id: &str| r.stations.iter().find(|x| x.id == id).unwrap().tt_k;
        let h_air = |tt: f64| CP_AIR * (tt - T_REF);
        let h_gas = |tt: f64| CP_GAS * (tt - T_REF);
        // Everything leaving the HPC: burner air plus cooling air at T3, plus the fuel
        let w_in = r.w25_kg_s - r.bleed_kg_s;
        let e_in = w_in * h_air(t("3")) + r.wf_kg_s * dp.eta_burner * dp.fuel_lhv_j_kg;
        // Everything at the LPT inlet, plus the work the HPT took out
        let e_out = r.w5_kg_s * h_gas(t("45")) + r.hpt_power_w;
        assert!((e_in - e_out).abs() / e_in < 1e-9, "{}: energy residual {:.0} W", s.name, e_in - e_out);
    }
}

#[test]
fn turbofan_mass_balance_closes_with_the_vbv_open() {
    let s = EngineSpec::cfm56_7b26();
    let d = size_design_point(&s);
    let inl = inlet(isa(0.0, 0.0), 0.0);
    let n2c = interp(&s.spools.n2_vs_n1, 21.0) / 100.0;
    let vbv = interp(&s.bleed.vbv_open_vs_n2c, n2c * 100.0);
    assert!(vbv > 0.5, "the VBVs should be open at idle ({vbv})");
    let r = steady_point(&s, &inl, CycleInput { vbv_open: vbv, bleed_fraction: 0.05, ..CycleInput::steady(0.21, n2c, 0.0, None) }, d.core_nozzle_area_m2);
    let w = |id: &str| r.stations.iter().find(|x| x.id == id).unwrap().w_kg_s;
    let resid = w("2") + r.wf_kg_s - w("9") - w("19") - r.bleed_kg_s;
    assert!(resid.abs() < 1e-6, "inlet {:.3} + fuel {:.3} vs nozzles {:.3} + {:.3} + bleed {:.3}: residual {:.4} kg/s (vbv {:.2})", w("2"), r.wf_kg_s, w("9"), w("19"), r.bleed_kg_s, resid, r.vbv_dump_kg_s);
}

#[test]
fn unpowered_turbofan_produces_drag_not_thrust() {
    for s in turbofans() {
        let d = size_design_point(&s);
        let inl = inlet(isa(10_668.0, 0.0), 0.78);
        for (n1c, n2c) in [(0.1, 0.2), (0.25, 0.35), (0.4, 0.5)] {
            let r = cycle(&s, &inl, CycleInput { combustion: false, ..CycleInput::steady(n1c, n2c, 0.0, Some(d.core_nozzle_area_m2)) });
            assert!(r.net_thrust_n < 0.0, "{}: cold cycle at N1c {n1c} gives {:.0} N", s.name, r.net_thrust_n);
            assert!(r.fan_power_w == 0.0 && r.hpt_power_w == 0.0, "{}: shaft work without combustion", s.name);
        }
        // Full engine model: fuel off, starter off, 600 s at cruise
        let mut e = Engine::new(s.clone());
        e.environment.altitude_m = 10_668.0;
        e.environment.mach = 0.78;
        run(&mut e, 600.0);
        let st = e.state();
        assert!(st.thrust_n < 0.0, "{}: windmilling at N1 {:.1} gives {:.0} N", s.name, st.n1_pct, st.thrust_n);
    }
}

#[test]
fn unpowered_turbojet_and_turboprop_cores_produce_no_jet_thrust() {
    let tj = TurbojetSpec::j85_ge_5a();
    let z = tj_size(&tj);
    let inl = inlet(isa(6_000.0, 0.0), 0.6);
    let r = tj_cycle(&tj, &inl, 0.3, 0.0, false, 0.0, z.a8_mil_m2, 0.0);
    assert!(r.net_thrust_n < 0.0, "turbojet windmill thrust {:.0} N", r.net_thrust_n);
    let tp = TurbopropSpec::pt6a_114a();
    let r = tp_cycle(&tp, &inlet(isa(3_000.0, 0.0), 0.3), 0.15, 0.0, false, 1.0, 0.0);
    assert!(r.jet_thrust_n <= 0.0 && r.shaft_power_w == 0.0, "turboprop windmill jet thrust {:.0} N", r.jet_thrust_n);
    // Static, where ram drag cannot hide an exhaust pressure rise: nothing leaves
    let r = tp_cycle(&tp, &inlet(isa(0.0, 0.0), 0.0), 0.15, 0.0, false, 1.0, 0.0);
    assert!(r.pt5 < inlet(isa(0.0, 0.0), 0.0).ambient.pressure_pa, "cold exhaust pressure {:.0} Pa above ambient", r.pt5);
    assert_eq!(r.jet_thrust_n, 0.0, "cold static turboprop jet thrust {:.1} N", r.jet_thrust_n);
}

#[test]
fn turboprop_jet_thrust_is_net_of_inlet_momentum() {
    let tp = TurbopropSpec::pt6a_114a();
    let z = tp_size(&tp);
    let inl = inlet(isa(3_048.0, 0.0), 0.25);
    let r = tp_cycle(&tp, &inl, 0.95, z.t4_takeoff_k * 0.95, true, 1.0, 0.0);
    let w45 = r.w2 + r.wf; // cooling air rejoins before the power turbine, no customer bleed here
    let gross = w45 * r.v9;
    let expected = gross - r.w2 * inl.true_airspeed_m_s;
    assert!((r.jet_thrust_n - expected).abs() < 1.0, "jet thrust {:.1} vs {:.1} N (gross {:.1})", r.jet_thrust_n, expected, gross);
}

#[test]
fn egt_limit_does_not_depend_on_the_order_bleeds_were_switched() {
    let s = EngineSpec::cfm56_7b26();
    let mut seq = Engine::new(s.clone());
    seq.environment.delta_isa_k = 40.0;
    seq.controls.anti_ice = true;
    seq.set_running(1.0);
    run(&mut seq, 5.0);
    seq.controls.pack_bleed = true;
    run(&mut seq, 60.0);
    let mut both = Engine::new(s);
    both.environment.delta_isa_k = 40.0;
    both.controls.anti_ice = true;
    both.controls.pack_bleed = true;
    both.set_running(1.0);
    run(&mut both, 65.0);
    let (a, b) = (seq.state(), both.state());
    assert!((a.n1_limit_pct - b.n1_limit_pct).abs() < 0.01, "N1 limit {:.2} vs {:.2}", a.n1_limit_pct, b.n1_limit_pct);
    assert!((a.egt_c - b.egt_c).abs() < 2.0, "EGT {:.1} vs {:.1} C", a.egt_c, b.egt_c);
}

#[test]
fn flight_idle_is_reached_at_cruise_altitude() {
    for s in turbofans() {
        for disa in [0.0, -30.0, 20.0] {
            let mut e = Engine::new(s.clone());
            e.environment.altitude_m = 10_668.0;
            e.environment.mach = 0.78;
            e.environment.delta_isa_k = disa;
            e.set_running(0.8);
            run(&mut e, 30.0);
            e.controls.tla = 0.0;
            run(&mut e, 120.0);
            let st = e.state();
            // The fan cannot turn slower than it windmills at this Mach number
            let n1_windmill = (s.start.windmill_n1_per_mach * 0.78).min(35.0);
            let at_floor = st.n1_pct <= n1_windmill + 0.05 && st.n1_command_pct < st.n1_pct;
            assert!((st.n1_pct - st.n1_command_pct).abs() < 0.5 || at_floor, "{} at ISA{disa:+}: commanded N1 {:.2}, actual {:.2} (fuel floor {:.3} kg/s)", s.name, st.n1_command_pct, st.n1_pct, st.fuel_decel_limit_kg_s);
        }
    }
}

#[test]
fn propeller_torque_is_negative_when_windmilling() {
    let p = PistonSpec::lycoming_o360().propeller;
    let f = prop_forces(&p, 1.225, 340.0, 60.0, 1_000.0, p.fixed_pitch_deg.unwrap());
    assert!(f.thrust_n < 0.0, "thrust {:.0} N", f.thrust_n);
    assert!(f.torque_nm < 0.0 && f.power_w < 0.0, "windmilling torque {:.0} N m should drive the shaft", f.torque_nm);
    // A dead engine windmills at airspeed from the aerodynamic torque alone
    let mut e = PistonEngine::new(PistonSpec::lycoming_o360());
    e.environment.tas_m_s = 60.0;
    for _ in 0..600 {
        e.step(0.05);
    }
    let s = e.state();
    assert!(s.rpm > 500.0, "dead engine at 60 m/s: {:.0} rpm", s.rpm);
    assert!(s.thrust_n < 0.0, "windmilling drag {:.0} N", s.thrust_n);
    // and stays stopped on the ground
    let mut g = PistonEngine::new(PistonSpec::lycoming_o360());
    for _ in 0..200 {
        g.step(0.05);
    }
    assert_eq!(g.state().rpm, 0.0);
}

#[test]
fn piston_both_tanks_conserve_fuel_when_one_is_empty() {
    let mut e = PistonEngine::new(PistonSpec::lycoming_o360());
    e.set_running(0.8);
    e.controls.fuel_selector = 3;
    e.controls.fuel_left_gal = 0.0;
    e.controls.fuel_right_gal = 10.0;
    for _ in 0..1_200 {
        e.step(0.05);
    }
    let used = e.state().fuel_used_gal;
    let drawn = 10.0 - e.controls.fuel_right_gal - e.controls.fuel_left_gal;
    assert!(used > 0.1, "engine should have burned fuel ({used:.3} gal)");
    assert!((used - drawn).abs() < 1e-9, "burned {used:.6} gal but the tanks lost {drawn:.6} gal");
}
